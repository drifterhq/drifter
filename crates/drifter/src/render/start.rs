use super::*;

pub(super) fn wire(
    handle: &smithay::reexports::calloop::LoopHandle<'static, State>,
    state: &State,
    drm_events: smithay::backend::drm::DrmDeviceNotifier,
    libinput: Libinput,
    plugs: std::io::Result<smithay::backend::udev::UdevBackend>,
    seat_events: LibSeatSessionNotifier,
) -> Result<(), String> {
    handle
        .insert_source(drm_events, |event, _, state: &mut State| match event {
            DrmEvent::VBlank(crtc) => {
                let Some(head) = state.heads.iter_mut().find(|head| head.crtc == crtc) else {
                    return;
                };
                head.awaiting = false;
                if let Err(error) = head.compositor.frame_submitted() {
                    log!("drifter: frame submit on {} failed: {error}", head.name);
                }
                if let Err(error) = state.draw() {
                    state.stop(error);
                }
            }
            DrmEvent::Error(error) => log!("drifter: drm event: {error}"),
        })
        .map_err(|e| format!("cannot watch drm events: {e}"))?;

    let card = state.drm.as_ref().map(|drm| drm.device_id());
    match plugs {
        Ok(plugs) => {
            handle
                .insert_source(plugs, move |event, _, state: &mut State| {
                    if let smithay::backend::udev::UdevEvent::Changed { device_id } = event
                        && Some(device_id) == card
                        && state.active
                    {
                        state.scan_heads();
                        state.kick_now();
                    }
                })
                .map_err(|e| format!("cannot watch for monitors: {e}"))?;
        }
        Err(error) => log!("drifter: monitors plugged in later are not seen: {error}"),
    }

    handle
        .insert_source(seat_events, |event, _, state: &mut State| match event {
            SessionEvent::PauseSession => {
                state.active = false;
                if let Some(libinput) = state.libinput.as_mut() {
                    libinput.suspend();
                }
                if let Some(drm) = state.drm.as_mut() {
                    drm.pause();
                }
                state.drop_held_input();
            }
            SessionEvent::ActivateSession => {
                state.active = true;
                state.dark = false;
                state.last_input = Instant::now();
                if state
                    .libinput
                    .as_mut()
                    .is_some_and(|libinput| libinput.resume().is_err())
                {
                    log!("drifter: cannot take the input devices back");
                }
                state.drop_held_input();
                if let Some(Err(error)) = state.drm.as_mut().map(|drm| drm.activate(true)) {
                    log!("drifter: cannot take the screen back: {error}");
                }
                for head in &mut state.heads {
                    head.awaiting = false;
                    head.warmth = None;
                    if let Err(error) = head.compositor.reset_state() {
                        log!("drifter: cannot reset {}: {error}", head.name);
                    }
                    head.compositor.reset_buffers();
                }
                state.scan_heads();
                state.last = Instant::now();
                state.kick();
            }
        })
        .map_err(|e| format!("cannot watch seat events: {e}"))?;

    handle
        .insert_source(
            LibinputInputBackend::new(libinput),
            |event, _, state: &mut State| {
                state.input(event);
                state.kick();
            },
        )
        .map_err(|e| format!("cannot watch input events: {e}"))?;
    Ok(())
}

pub struct Seat<'a> {
    pub session: &'a mut LibSeatSession,
    pub events: LibSeatSessionNotifier,
    pub card: &'a Path,
}

pub fn open(
    seat: Option<Seat<'_>>,
    config: &Config,
    virtual_size: (u32, u32),
    background: [f32; 4],
    wallpaper: Option<Image>,
) -> Result<Screen, String> {
    let events: EventLoop<'static, State> =
        EventLoop::try_new().map_err(|e| format!("cannot create the event loop: {e}"))?;
    let live = seat.is_some();
    let signals = match live {
        true => Some(
            Signals::new(&[
                Signal::SIGUSR1,
                Signal::SIGUSR2,
                Signal::SIGTERM,
                Signal::SIGHUP,
                Signal::SIGINT,
            ])
            .map_err(|e| format!("cannot watch signals: {e}"))?,
        ),
        false => None,
    };

    let mut path = PathBuf::from("headless");
    let mut drm = None;
    let mut drm_events = None;
    let mut gbm = None;
    let mut render_node = None;
    let mut wired = None;
    let vt = seat.as_ref().map(|seat| seat.session.clone());
    if let Some(seat) = seat {
        path = seat.card.to_path_buf();
        let fd = seat
            .session
            .open(
                seat.card,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(|e| format!("cannot open {}: {e}", seat.card.display()))?;

        let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
        render_node = smithay::backend::drm::DrmNode::from_file(drm_fd.clone())
            .ok()
            .map(|node| {
                node.node_with_type(smithay::backend::drm::NodeType::Render)
                    .and_then(Result::ok)
                    .unwrap_or(node)
            });
        gbm = GbmDevice::new(drm_fd.clone())
            .inspect_err(|e| log!("drifter: no gbm device: {e}"))
            .ok();
        let (device, notifier) =
            DrmDevice::new(drm_fd, true).map_err(|e| format!("cannot open drm device: {e}"))?;
        drm = Some(device);
        drm_events = Some(notifier);

        let mut libinput =
            Libinput::new_with_udev(LibinputSessionInterface::from(seat.session.clone()));
        let seat_name = seat.session.seat();
        libinput
            .udev_assign_seat(&seat_name)
            .map_err(|_| format!("cannot take input on seat {seat_name}"))?;
        let plugs = smithay::backend::udev::UdevBackend::new(&seat_name);
        wired = Some((libinput, plugs, seat.events));
    }

    let (mut renderer, renderer_formats, gl) = pick_painter(gbm.as_ref())?;
    let screen = Vec2::new(f64::from(virtual_size.0), f64::from(virtual_size.1));
    let world = config.world_size(screen);
    let start = Camera::at_work_zoom(world / 2.0);
    let tunables = match config.access.reduce_motion {
        true => Tunables::from(&config.motion).still(),
        false => Tunables::from(&config.motion),
    };
    crate::apps::set_env("DRIFTER_RENDERER", Some(gl.clone()));
    drifter_core::icon::set_theme(config.appearance.icons.clone());
    let style = drifter_core::theme::now(config.appearance.style, &config.night);
    tell_gnome(None, &config.appearance, Some(style));
    tell_buttons(config.window.gtk_layout());
    dress_gtk4();
    tell_access(None, &config.access);
    cursor_env(&config.appearance);
    let shapes = [
        Panel {
            height: config.drawers.top,
            ..Panel::default()
        },
        Panel {
            height: config.drawers.bottom,
            ..Panel::default()
        },
    ];

    let shader = config
        .wallpaper_path()
        .filter(|path| drifter_core::shader::is_shader(path));
    let wallpaper_element = wallpaper.map(|image| {
        let live = shader
            .as_deref()
            .and_then(|path| super::wallpaper::Live::compile(&mut renderer, path));
        Backdrop::new(
            drifter_core::tiles::Pyramid::build(image, world, screen),
            live,
            shader.is_some(),
        )
    });
    let (arrow, arrow_size, arrow_hotspot) = load_arrow(&mut renderer, config)?;

    let open_band = {
        let mut probe = Drawer::new(Edge::Top);
        probe.open = true;
        probe.advance(Duration::from_secs(1), Duration::from_millis(1));
        probe.band(screen, &shapes[Edge::Top.index()])
    };

    let palette = drifter_core::theme::of(style);
    let (skin, mut chrome) = load_skin(&mut renderer, config, &palette);
    chrome.follow(&config.window);

    let panels = paint_panels(&mut renderer, screen.x, &shapes, &palette)?;
    let drops = paint_drops(&mut renderer, screen.x, &shapes, &palette);

    let tray = Tray {
        size: config.tray.size,
        ink: palette.ink,
        ..Tray::default()
    };
    let display: Display<State> =
        Display::new().map_err(|e| format!("cannot start the wayland display: {e}"))?;
    let mut wayland = crate::shell::start(&display, &config.keyboard, events.handle())?;
    {
        use smithay::backend::renderer::ImportDma;
        use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

        let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
        let feedback = render_node.and_then(|node| {
            DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
                .build()
                .ok()
        });
        let handle = display.handle();
        let dmabuf = &mut wayland.extras.dmabuf;
        let global = match feedback {
            Some(feedback) => {
                dmabuf.create_global_with_default_feedback::<State>(&handle, &feedback)
            }
            None => dmabuf.create_global::<State>(&handle, formats),
        };
        wayland.extras.dmabuf_global = Some(global);
    }
    let output = smithay::output::Output::new(
        "none".to_string(),
        smithay::output::PhysicalProperties {
            size: (0, 0).into(),
            subpixel: smithay::output::Subpixel::Unknown,
            make: "Drifter".to_string(),
            model: "Drifter".to_string(),
        },
    );

    let (tray_bus, tray_icons) = match live.then(crate::tray::start) {
        Some(Ok((bus, icons))) => (Some(bus), Some(icons)),
        Some(Err(error)) => {
            log!("drifter: no tray: {error}");
            (None, None)
        }
        None => (None, None),
    };
    let (notes_bus, note_events) = match live.then(crate::notes::start) {
        Some(Ok((bus, events))) => (Some(bus), Some(events)),
        Some(Err(error)) => {
            log!("drifter: no notifications: {error}");
            (None, None)
        }
        None => (None, None),
    };
    let (media, media_readings) = match live.then(crate::media::start) {
        Some(Ok((media, readings))) => (Some(media), Some(readings)),
        Some(Err(error)) => {
            log!("drifter: no media keys: {error}");
            (None, None)
        }
        None => (None, None),
    };
    let font = match Font::find(config.font_path(), tray.size) {
        Ok(font) => Some(std::sync::Arc::new(font)),
        Err(error) => {
            log!("drifter: no clock: {error}");
            None
        }
    };

    let trash_look = Dock::themed(&palette).sized(config.drawers.bottom);
    let side = trash_look.icon as u32;
    let (trash, cog, spare) = load_dock_art(&mut renderer, config, side, &palette);
    let shadow_image = drifter_core::window::shadow_patch(chrome.radius, drifter_core::window::SHADOW_BLUR);
    let window_shadow = upload_flat(&mut renderer, Some(&shadow_image), "window shadow")?
        .map(|buffer| (buffer, f64::from(shadow_image.width - 1) / 2.0));
    let control_icon = upload(
        &mut renderer,
        Some(&drifter_core::control::paint_icon(tray.button as u32, tray.ink)),
        "control center button",
    )?;
    let power_image = drifter_core::tray::paint_power(tray.button as u32, tray.ink);
    let power = upload(&mut renderer, Some(&power_image), "power")?;
    let lock_big = Font::find(config.font_path(), 136.0).ok().map(std::sync::Arc::new);
    let lock_small = Font::find(config.font_path(), 17.0).ok().map(std::sync::Arc::new);
    let lock_name = Font::find(config.font_path(), 26.0).ok().map(std::sync::Arc::new);
    let (checked_tx, checked_rx) =
        smithay::reexports::calloop::channel::channel::<(String, crate::polkit::Outcome)>();
    let (paper_tx, paper_rx) = smithay::reexports::calloop::channel::channel::<(
        PathBuf,
        Option<drifter_core::tiles::Pyramid>,
    )>();
    let (chores, chores_rx) = smithay::reexports::calloop::channel::channel::<Chore>();
    let (shot_tx, shot_rx) =
        smithay::reexports::calloop::channel::channel::<Result<PathBuf, String>>();
    let (verify_tx, verify_rx) =
        smithay::reexports::calloop::channel::channel::<(u64, crate::auth::Verdict)>();

    let pinned = dock_items(&mut renderer, config, side, spare.as_ref())?;

    let dot_image = drifter_core::dock::paint_dot(trash_look.dot_side() as u32, palette.ink);
    let dot = upload(&mut renderer, Some(&dot_image), "running dot")?;
    let glow_image =
        drifter_core::corner::paint_glow(drifter_core::corner::REACH as u32, [255, 214, 160]);
    let glow = upload(&mut renderer, Some(&glow_image), "hot corner")?;

    let dock_look = Dock::themed(&palette);
    let rule_image = drifter_core::Image {
        width: 1,
        height: 1,
        rgba: vec![
            dock_look.rule[0] / 2,
            dock_look.rule[1] / 2,
            dock_look.rule[2] / 2,
            128,
        ],
    };
    let rule = upload(&mut renderer, Some(&rule_image), "dock divider")?;
    let plain_image = drifter_core::Image {
        width: 1,
        height: 1,
        rgba: vec![255, 255, 255, 255],
    };
    let plain = upload(&mut renderer, Some(&plain_image), "outline")?;

    let (clip, clip_events) = crate::clipboard::Keeper::new(events.handle());
    let state = State {
        output,
        wayland,
        windows: Vec::new(),
        popups: smithay::desktop::PopupManager::default(),
        display: Some(display),
        chrome,
        camera: start,
        world,
        mode: config.workspace.wallpaper_mode,
        flight: None,
        corner_since: None,
        grab: Grab::None,
        modifier: false,
        fingers: 0,
        touch: config.touchpad,
        touchpads: Vec::new(),
        mouse: config.mouse,
        mice: Vec::new(),
        power_rules: config.power,
        slept: false,
        notify: config.notify,
        noted: Vec::new(),
        shortcuts: Vec::new(),
        switch_mods: drifter_core::shortcut::Mods::default(),
        swallowed: Vec::new(),
        appearance: config.appearance.clone(),
        style,
        gtk_layout: config.window.gtk_layout(),
        asset_paths: asset_paths(config),
        holds: config.input.modifier,
        last_title: None,
        held: None,
        focus: None,
        next_window: 0,
        eaten: Vec::new(),
        frame_timer: false,
        cursor_status: smithay::input::pointer::CursorImageStatus::default_named(),
        named_cursors: std::collections::HashMap::new(),
        layer_focus: None,
        skin,
        started: Instant::now(),
        socket: String::new(),
        drm,
        gbm,
        render_node,
        render_formats: renderer_formats,
        virtual_size,
        monitors: config.monitor.clone(),
        screens_wide: (config.workspace.width, config.workspace.height),
        heads: Vec::new(),
        plugs: Vec::new(),
        at: 0,
        hand: 0,
        libinput: wired.as_ref().map(|(libinput, _, _)| libinput.clone()),
        renderer,
        wallpaper: wallpaper_element,
        panels,
        shapes,
        arrow,
        arrow_size,
        arrow_hotspot,
        font,
        tray,
        dock: Dock {
            settings: config.dock.settings,
            trash: config.dock.trash,
            ..Dock::themed(&palette).sized(config.drawers.bottom)
        },
        pinned,
        pinned_from: (config.dock.clone(), config.apps.clone()),
        apps_chosen: config.apps.clone(),
        dock_spare: spare,
        trash,
        cog,
        dot,
        glow,
        rule,
        app_icons: std::collections::HashMap::new(),
        no_icon: std::collections::HashSet::new(),
        format: config.tray.format.clone(),
        clock: None,
        stats: None,
        sampler: crate::stats::Sampler::new(&config.tray),
        sampled: None,
        icons: Vec::new(),
        popover: None,
        desk_menu: None,
        ripple: None,
        alert: None,
        reopening: None,
        switch: None,
        lock: None,
        client_lock: None,
        volume: None,
        volume_read: None,
        speaker: None,
        light: None,
        light_read: None,
        sun: None,
        gauge_panel: None,
        lock_big,
        lock_small,
        lock_name,
        verify: verify_tx,
        tickets: 0,
        power,
        look: Look {
            accent: config
                .appearance
                .accent
                .map(|accent| accent.color())
                .unwrap_or(drifter_core::menu::ACCENT),
            ..Look::themed(&palette)
        },
        palette,
        tray_raw: Vec::new(),
        frost_key: None,
        fading: None,
        control: None,
        control_icon,
        control_status: drifter_core::control::Status::default(),
        control_stale: false,
        gauge_stale: false,
        notes_panel: None,
        search: None,
        launchpad: None,
        welcome: None,
        dock_logo: docklogo::DockLogo::default(),
        lock_logo: docklogo::DockLogo::default(),
        welcome_wanted: live && config.welcome.show,
        login_lock: live && !config.login.automatic,
        search_font: Font::find(config.font_path(), 20.0).ok().map(std::sync::Arc::new),
        search_icons: std::collections::HashMap::new(),
        app_label: None,
        window_shadow,
        bounces: Vec::new(),
        dock_press: None,
        dock_drag: None,
        trash_full: false,
        trash_seen: None,
        trash_full_art: None,
        dock_art_dir: config.dock_dir(),
        badges: std::collections::HashMap::new(),
        badge_art: std::collections::HashMap::new(),
        drops,
        frost_at: None,
        frost_drift: None,
        frost_busy: false,
        reused: std::collections::HashMap::new(),
        installed: None,
        app_names: std::cell::RefCell::new(std::collections::HashMap::new()),
        resizing: None,
        density: 1,
        tip: None,
        tip_of: None,
        tip_want: None,
        tip_lane: worker::Lane::default(),
        notes_lane: worker::Lane::default(),
        hud_lane: worker::Lane::default(),
        hud_want: None,
        prompt_lane: worker::Lane::default(),
        haze_lane: worker::Lane::default(),
        icon_jobs: worker::Icons::default(),
        snap: Snap::default(),
        config_seen: None,
        last_input: Instant::now(),
        keys: config.keyboard.clone(),
        night: config.night,
        lock_after: config.lock.after,
        screen_off: config.lock.screen_off,
        dark: false,
        login: config.login.start.clone(),
        autostart: config.login.autostart,
        vt,
        paper: Paper::from(config),
        paper_tx,
        shot_tx,
        chores,
        config_path: match live {
            true => Config::config_path(),
            false => PathBuf::new(),
        },
        zone: None,
        plain,
        titles: std::collections::HashMap::new(),
        hover_since: None,
        tray_bus,
        media,
        hud: None,
        notes_bus,
        cards: Vec::new(),
        copies: Vec::new(),
        prompts: Vec::new(),
        prompt_art: None,
        prompt_haze: None,
        checked: checked_tx,
        copied: std::collections::HashSet::new(),
        cursor_count: 0,
        shot_wanted: false,
        shot_crop: None,
        shot_tool: drifter_core::grab::Tool::new(
            drifter_core::grab::Take::Still,
            drifter_core::grab::Pick::Portion,
        ),
        shot_inks: None,
        shooter: None,
        recording: None,
        thumb: None,
        sound_from: None,
        own_count: 0,
        drawers: [Drawer::new(Edge::Top), Drawer::new(Edge::Bottom)],
        tunables,
        overview_hold: None,
        places: Vec::new(),
        place_saves: Vec::new(),
        note_folders: std::collections::HashMap::new(),
        access: config.access,
        sticky: drifter_core::sticky::Sticky::default(),
        shots: config.screenshots.clone(),
        screen,
        pointer: screen / 2.0,
        dwell: None,
        leave: None,
        last: Instant::now(),
        clear: Color32F::from(background),
        active: true,
        ticking: false,
        handle: events.handle(),
        clip,
        signal: events.get_signal(),
        failure: None,
    };

    let mut state = state;
    state.refresh_apps();
    state.set_shortcuts(&config.keys, config.input.modifier);
    state.set_places(&config.place, config.input.modifier);
    match live {
        true => state.scan_heads(),
        false => state.add_offscreen_head(virtual_size)?,
    }
    state.follow_density();
    if state.heads.is_empty() {
        return Err("no connected monitor with modes".to_string());
    }
    state.heads[0].camera = Camera::at_work_zoom(state.world / 2.0);
    for index in 1..state.heads.len() {
        state.heads[index].camera = state.beside(index);
    }
    state.load(state.hand);
    state.pointer = state.screen / 2.0;
    let first = &state.heads[0];
    let found_name = state
        .heads
        .iter()
        .map(|head| head.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let found_size = (first.pixels.x as u16, first.pixels.y as u16);
    let found_refresh = (first.mode.refresh / 1000) as u32;
    let rect = state.wallpaper.as_ref().map(|paper| {
        drifter_core::wallpaper::place(
            config.workspace.wallpaper_mode,
            paper.pyramid.size(),
            state.screen,
            state.world,
            &state.camera,
        )
        .target
    });
    let start = state.camera;

    let handle = events.handle();

    if let (Some(drm_events), Some((libinput, plugs, seat_events))) = (drm_events, wired) {
        wire(&handle, &state, drm_events, libinput, plugs, seat_events)?;
    }

    if let Some(icons) = tray_icons {
        handle
            .insert_source(icons, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(update) = event {
                    match update {
                        crate::tray::Update::Icons(icons) => state.take_icons(icons),
                        crate::tray::Update::Menu {
                            service,
                            trail,
                            items,
                            at,
                        } => {
                            let at = Vec2::new(at.0, at.1);
                            let items = match trail.is_empty() {
                                true => items,
                                false => drifter_core::menu::with_back(items),
                            };
                            state.open_popover(items, at, Target::Tray { service, trail, at });
                        }
                    }
                    state.kick();
                }
            })
            .map_err(|e| format!("cannot watch the tray: {e}"))?;
    }

    if let Some(events) = note_events {
        handle
            .insert_source(events, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(event) = event {
                    match event {
                        crate::notes::Event::Show(note) => state.show_note(note),
                        crate::notes::Event::Close(id) => state.close_note(id, 3),
                    }
                }
            })
            .map_err(|e| format!("cannot watch the notifications: {e}"))?;
    }

    if live {
        match crate::launcher::start() {
            Ok(badges) => {
                handle
                    .insert_source(badges, |event, _, state: &mut State| {
                        if let ChannelEvent::Msg(heard) = event {
                            state.take_badge(
                                heard.app,
                                dock::Badge {
                                    count: heard.count,
                                    progress: heard.progress,
                                    urgent: heard.urgent,
                                },
                            );
                        }
                    })
                    .map_err(|e| format!("cannot watch the dock badges: {e}"))?;
            }
            Err(error) => log!("drifter: {error}"),
        }
    }

    if let Some(readings) = media_readings {
        handle
            .insert_source(readings, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(heard) = event {
                    state.hear(heard);
                }
            })
            .map_err(|e| format!("cannot watch the media keys: {e}"))?;
    }

    handle
        .insert_source(checked_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((cookie, outcome)) = event {
                state.prompt_checked(&cookie, outcome);
            }
        })
        .map_err(|e| format!("cannot watch the password checks: {e}"))?;
    handle
        .insert_source(clip_events, |event, _, state: &mut State| {
            if let ChannelEvent::Msg(copied) = event {
                let seat = state.wayland._seat.clone();
                let dh = state.wayland.handle.clone();
                state.clip.arrived(copied, &dh, &seat);
            }
        })
        .map_err(|e| format!("cannot keep the clipboard: {e}"))?;

    match live
        .then(crate::logind::start)
        .unwrap_or(Err(String::new()))
    {
        Ok(events) => {
            handle
                .insert_source(events, |event, _, state: &mut State| {
                    if let ChannelEvent::Msg(event) = event {
                        match event {
                            crate::logind::Event::Lock | crate::logind::Event::Sleep => {
                                state.lock_now();
                            }
                            crate::logind::Event::Woke => {
                                state.last_input = Instant::now();
                                state.slept = false;
                                state.wake();
                            }
                        }
                    }
                })
                .map_err(|e| format!("cannot watch logind: {e}"))?;
        }
        Err(error) if live => log!("drifter: no lock on sleep or on request: {error}"),
        Err(_) => {}
    }

    match live
        .then(crate::disks::start)
        .unwrap_or(Err(String::new()))
    {
        Ok(events) => {
            handle
                .insert_source(events, |event, _, state: &mut State| {
                    if let ChannelEvent::Msg(event) = event {
                        state.disk_event(event);
                    }
                })
                .map_err(|e| format!("cannot watch disks: {e}"))?;
        }
        Err(error) if live => log!("drifter: plugged in disks are not opened: {error}"),
        Err(_) => {}
    }

    match live
        .then(crate::polkit::start)
        .unwrap_or(Err(String::new()))
    {
        Ok(requests) => {
            handle
                .insert_source(requests, |event, _, state: &mut State| {
                    if let ChannelEvent::Msg(event) = event {
                        match event {
                            crate::polkit::Event::Begin(request) => state.prompt_begin(request),
                            crate::polkit::Event::Cancel(cookie) => {
                                state.prompt_end(&cookie, false)
                            }
                        }
                    }
                })
                .map_err(|e| format!("cannot watch polkit: {e}"))?;
        }
        Err(error) if live => log!("drifter: no password prompt: {error}"),
        Err(_) => {}
    }

    handle
        .insert_source(shot_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg(saved) = event {
                state.shot_saved(saved);
            }
        })
        .map_err(|e| format!("cannot watch screenshots: {e}"))?;

    handle
        .insert_source(chores_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg(chore) = event {
                chore(state);
            }
        })
        .map_err(|e| format!("cannot watch background work: {e}"))?;

    handle
        .insert_source(paper_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((path, pyramid)) = event {
                state.take_wallpaper(path, pyramid);
            }
        })
        .map_err(|e| format!("cannot watch the wallpaper: {e}"))?;

    handle
        .insert_source(verify_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((ticket, verdict)) = event {
                state.verdict(ticket, verdict);
                state.kick();
            }
        })
        .map_err(|e| format!("cannot watch the lock: {e}"))?;

    let socket = smithay::wayland::socket::ListeningSocketSource::new_auto()
        .map_err(|e| format!("cannot open a wayland socket: {e}"))?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    handle
        .insert_source(socket, |stream, _, state: &mut State| {
            if let Err(error) = state
                .wayland
                .handle
                .insert_client(stream, crate::shell::client_data())
            {
                log!("drifter: cannot greet a client: {error}");
            }
        })
        .map_err(|e| format!("cannot watch the wayland socket: {e}"))?;

    state.socket = socket_name.clone();
    if live {
        crate::apps::start_x11(&socket_name);
        crate::portal::announce(&socket_name);
    }
    let poll = state_poll_fd(&mut state);
    handle
        .insert_source(poll, |_, _, state: &mut State| {
            state.dispatch_clients();
            state.kick_now();
            Ok(smithay::reexports::calloop::PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch wayland clients: {e}"))?;

    state.config_seen = config_stamp(&state.config_path);
    handle
        .insert_source(
            Timer::from_duration(CONFIG_WATCH),
            |_, _, state: &mut State| {
                state.watch_config();
                state.watch_idle();
                state.watch_night();
                state.watch_style();
                state.evict_tiles();
                state.watch_wallpaper();
                state.watch_trash();
                TimeoutAction::ToDuration(CONFIG_WATCH)
            },
        )
        .map_err(|e| format!("cannot watch the config: {e}"))?;

    if let Some(signals) = signals {
        handle
            .insert_source(signals, |event, _, state: &mut State| {
                let edge = match event.signal() {
                    Signal::SIGUSR1 => Edge::Top,
                    Signal::SIGUSR2 => Edge::Bottom,
                    _ => {
                        state.signal.stop();
                        return;
                    }
                };
                state.toggle(edge);
                state.kick();
            })
            .map_err(|e| format!("cannot watch signals: {e}"))?;
    }

    Ok(Screen {
        found: Found {
            device: path.to_path_buf(),
            connector: found_name,
            width: found_size.0,
            height: found_size.1,
            refresh: found_refresh,
            renderer: gl,
            socket: socket_name.clone(),
            wallpaper: rect,
            camera: start,
            drawer: open_band,
            corner: shapes[Edge::Top.index()].radius,
        },
        events,
        state,
    })
}

pub(super) fn state_poll_fd(
    state: &mut State,
) -> smithay::reexports::calloop::generic::Generic<std::os::fd::OwnedFd> {
    use smithay::reexports::calloop::{Interest, Mode as PollMode, generic::Generic};
    let fd = state
        .display
        .as_mut()
        .expect("the display was just made")
        .backend()
        .poll_fd()
        .try_clone_to_owned()
        .expect("cannot clone the wayland poll fd");
    Generic::new(fd, Interest::READ, PollMode::Level)
}

pub(super) fn pick_painter(
    gbm: Option<&GbmDevice<DrmDeviceFd>>,
) -> Result<
    (
        Painter,
        smithay::backend::allocator::format::FormatSet,
        String,
    ),
    String,
> {
    let gles = gbm.ok_or_else(|| "no gbm".to_string()).and_then(gles);
    match gles {
        Ok((mut renderer, formats)) => {
            let name = describe(&mut renderer);
            if !is_software(&name) {
                return Ok((Painter::Gles(renderer), formats, name));
            }
            log!("drifter: {name} is software gl, so drifter draws with pixman");
        }
        Err(error) => log!("drifter: no gpu ({error}), so drifter draws with pixman"),
    }
    let renderer = smithay::backend::renderer::pixman::PixmanRenderer::new()
        .map_err(|e| format!("pixman renderer: {e}"))?;
    let painter = Painter::Pixman(renderer);
    let formats = painter.render_formats();
    Ok((painter, formats, "pixman".to_string()))
}

pub(super) fn gles(
    gbm: &GbmDevice<DrmDeviceFd>,
) -> Result<(GlesRenderer, smithay::backend::allocator::format::FormatSet), String> {
    let egl_display =
        unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| format!("egl display: {e}"))?;
    let context = EGLContext::new(&egl_display).map_err(|e| format!("egl context: {e}"))?;
    let formats = context.dmabuf_render_formats().clone();
    let renderer =
        unsafe { GlesRenderer::new(context) }.map_err(|e| format!("gles renderer: {e}"))?;
    Ok((renderer, formats))
}

pub(super) fn is_software(renderer: &str) -> bool {
    let renderer = renderer.to_lowercase();
    ["llvmpipe", "softpipe", "swrast"]
        .iter()
        .any(|name| renderer.contains(name))
}

pub(super) fn describe(renderer: &mut GlesRenderer) -> String {
    use smithay::backend::renderer::gles::ffi;
    renderer
        .with_context(|gl| unsafe {
            let raw = gl.GetString(ffi::RENDERER);
            if raw.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(raw as *const std::ffi::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .unwrap_or_else(|_| "unknown".to_string())
}
