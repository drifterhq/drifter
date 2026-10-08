use super::*;
use drifter_core::grab::{self, Pick, Slot, Take, Tool};

const THUMB_WIDE: f64 = 220.0;
const THUMB_TIME: Duration = Duration::from_secs(5);
const CROSS: u32 = 23;

pub(super) struct Shooter {
    pub tool: Tool,
    pub bar: Option<(TextureBuffer<Tex>, Rect)>,
    pub hover: Option<Slot>,
    pub from: Option<Vec2>,
    pub chosen: Option<Rect>,
    pub under: Option<Rect>,
    pub tag: Option<(TextureBuffer<Tex>, String, Vec2)>,
    pub tag_want: Option<String>,
    pub sources: Vec<grab::Source>,
    pub bar_lane: worker::Lane,
    pub tag_lane: worker::Lane,
}

pub(super) struct Recording {
    pub child: std::process::Child,
    pub path: PathBuf,
    pub began: Instant,
    pub art: Option<(TextureBuffer<Tex>, Rect)>,
    pub shown: u64,
    pub stopping: bool,
    pub lane: worker::Lane,
}

pub(super) struct Thumb {
    pub art: TextureBuffer<Tex>,
    pub rect: Rect,
    pub path: Option<PathBuf>,
    pub born: Instant,
    pub leaving: Option<Instant>,
    pub pressed: Option<Vec2>,
}

const THUMB_IN: f64 = 0.25;
const THUMB_OUT: f64 = 0.3;
const SHOT_COPY: i32 = 1;
const SHOT_FOLDER: i32 = 2;
const SHOT_OPEN: i32 = 3;
const SHOT_DELETE: i32 = 4;

pub(super) struct Inks {
    pub cross: TextureBuffer<Tex>,
    pub accent: TextureBuffer<Tex>,
    pub sides: Vec<TextureBuffer<Tex>>,
    pub shades: Vec<TextureBuffer<Tex>>,
}

impl State {
    pub(super) fn open_shooter(&mut self, tool: Tool, with_bar: bool) {
        if self.lock.is_some() {
            return;
        }
        self.popover = None;
        self.search = None;
        self.launchpad = None;
        self.control = None;
        self.notes_panel = None;
        self.glance_panel = None;
        self.shooter = Some(Shooter {
            tool,
            bar: None,
            hover: None,
            from: None,
            chosen: None,
            under: None,
            tag: None,
            tag_want: None,
            sources: Vec::new(),
            bar_lane: worker::Lane::default(),
            tag_lane: worker::Lane::default(),
        });
        self.shooter_inks();
        if with_bar {
            self.paint_shooter_bar();
        }
        self.aim_shooter();
        self.kick_now();
    }

    pub(super) fn shooter_inks(&mut self) {
        let fresh = self.shot_inks.as_ref().is_some_and(|(accent, _)| *accent == self.look.accent);
        if !fresh {
            let pixel = |rgb: [u8; 3]| drifter_core::Image {
                width: 1,
                height: 1,
                rgba: vec![rgb[0], rgb[1], rgb[2], 255],
            };
            let cross = grab::paint_cross(CROSS);
            let mut four = |rgb: [u8; 3], what: &str| -> Vec<TextureBuffer<Tex>> {
                (0..4)
                    .filter_map(|_| upload(&mut self.renderer, Some(&pixel(rgb)), what).ok().flatten())
                    .collect()
            };
            let sides = four([255, 255, 255], "selection edge");
            let shades = four([0, 0, 0], "selection shade");
            let made = (
                upload(&mut self.renderer, Some(&cross), "crosshair"),
                upload(&mut self.renderer, Some(&pixel(self.look.accent)), "accent"),
            );
            if let (Ok(Some(cross)), Ok(Some(accent)), 4, 4) = (made.0, made.1, sides.len(), shades.len()) {
                self.shot_inks = Some((self.look.accent, Inks {
                    cross,
                    accent,
                    sides,
                    shades,
                }));
            }
        }
    }

    fn bar_look(&self) -> grab::Bar {
        let plain = grab::Bar::themed(&self.palette, self.look.accent);
        grab::Bar {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    fn paint_shooter_bar(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let look = self.bar_look();
        let rect = look.rect(self.screen);
        let (tool, hover) = (shooter.tool, shooter.hover);
        let frosting = self.frosting();
        self.paint_in_lane(
            "screenshot bar",
            |state| state.shooter.as_mut().map(|shooter| &mut shooter.bar_lane),
            State::paint_shooter_bar,
            move || {
                let mut image = look.paint(tool, hover, &font);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            move |state, art| {
                if let Some(shooter) = state.shooter.as_mut() {
                    shooter.bar = Some((art, rect));
                }
            },
        );
    }

    fn frame_on_screen(&self, index: usize) -> Rect {
        let window = &self.windows[index];
        let chrome = self.chrome;
        let frame = match window.decorated && window.full.is_none() {
            true => Rect::new(
                window.content.origin - Vec2::new(chrome.edge, chrome.title),
                window.content.size + Vec2::new(chrome.edge * 2.0, chrome.title + chrome.edge),
            ),
            false => window.content,
        };
        let shown = self.on_screen(frame);
        grab::drag_rect(shown.origin, shown.max(), self.screen)
    }

    fn window_under_pointer(&self) -> Option<usize> {
        self.windows.iter().enumerate().rev().find_map(|(index, window)| {
            let usable = window.placed && !window.hidden && !window.going;
            (usable && self.frame_on_screen(index).contains(self.pointer)).then_some(index)
        })
    }

    pub(super) fn aim_shooter(&mut self) {
        let Some(tool) = self.shooter.as_ref().map(|shooter| shooter.tool) else {
            return;
        };
        let on_bar = self
            .shooter
            .as_ref()
            .and_then(|shooter| shooter.bar.as_ref())
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        let under = match (tool.pick, on_bar) {
            (Pick::Window, false) => self.window_under_pointer().map(|index| self.frame_on_screen(index)),
            _ => None,
        };
        let hover = match on_bar {
            true => self.shooter.as_ref().and_then(|shooter| {
                let (_, rect) = shooter.bar.as_ref()?;
                self.bar_look().hit(self.pointer - rect.origin)
            }),
            false => None,
        };
        let dragging = self
            .shooter
            .as_ref()
            .and_then(|shooter| shooter.from)
            .map(|from| grab::drag_rect(from, self.pointer, self.screen));
        let words = match (tool.pick, dragging) {
            (_, Some(rect)) => Some(grab::size_tag(rect)),
            (Pick::Portion, None) if !on_bar => {
                Some(format!("{}  {}", self.pointer.x.round(), self.pointer.y.round()))
            }
            _ => None,
        };
        let Some(shooter) = self.shooter.as_mut() else {
            return;
        };
        shooter.under = under;
        let stale = words.as_ref() != shooter.tag.as_ref().map(|(_, held, _)| held);
        if words.is_none() {
            shooter.tag = None;
        }
        shooter.tag_want = words;
        if stale {
            self.paint_tag();
        }
        let Some(shooter) = self.shooter.as_mut() else {
            return;
        };
        if shooter.hover != hover {
            shooter.hover = hover;
            self.paint_shooter_bar();
        }
        self.kick_now();
    }

    fn paint_tag(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(words) = self.shooter.as_ref().and_then(|shooter| shooter.tag_want.clone()) else {
            return;
        };
        let painted = words.clone();
        self.paint_in_lane_sized(
            "size tag",
            |state| state.shooter.as_mut().map(|shooter| &mut shooter.tag_lane),
            State::paint_tag,
            move || Some(grab::paint_tag(&painted, &font)),
            move |state, art, size| {
                if let Some(shooter) = state.shooter.as_mut()
                    && shooter.tag_want.as_deref() == Some(words.as_str())
                {
                    shooter.tag = Some((art, words, size));
                }
            },
        );
    }

    pub(super) fn shooter_key(&mut self, key: LockKey) {
        match key {
            LockKey::Clear => {
                self.shooter = None;
                self.popover = None;
            }
            LockKey::Letter(' ') => {
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.tool = shooter.tool.spaced();
                    shooter.from = None;
                    shooter.chosen = None;
                }
                if self.shooter.as_ref().is_some_and(|shooter| shooter.bar.is_some()) {
                    self.paint_shooter_bar();
                }
                self.aim_shooter();
            }
            LockKey::Enter => self.shoot(),
            LockKey::Vt(vt) => self.change_vt(vt),
            _ => {}
        }
        self.kick_now();
    }

    pub(super) fn press_shooter(&mut self, code: u32) -> bool {
        let Some(shooter) = self.shooter.as_ref() else {
            return false;
        };
        if code != BTN_LEFT {
            return true;
        }
        if let Some((_, rect)) = shooter.bar.as_ref()
            && rect.contains(self.pointer)
        {
            let rect = *rect;
            match self.bar_look().hit(self.pointer - rect.origin) {
                Some(Slot::Close) => self.shooter = None,
                Some(Slot::Tool(tool)) => {
                    if let Some(shooter) = self.shooter.as_mut() {
                        shooter.tool = tool;
                        if tool.pick != Pick::Portion {
                            shooter.chosen = None;
                        }
                    }
                    self.paint_shooter_bar();
                }
                Some(Slot::Options) => self.shooter_options(rect),
                Some(Slot::Go) => self.shoot(),
                None => {}
            }
            self.kick_now();
            return true;
        }
        match shooter.tool.pick {
            Pick::Screen => self.shoot(),
            Pick::Window => {
                if let Some(index) = self.window_under_pointer() {
                    self.raise(index);
                    let rect = self.frame_on_screen(index);
                    self.fire(Some(rect));
                }
            }
            Pick::Portion => {
                let at = self.pointer;
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.from = Some(at);
                    shooter.chosen = None;
                }
                self.aim_shooter();
            }
        }
        true
    }

    pub(super) fn release_shooter(&mut self) {
        let Some(from) = self.shooter.as_mut().and_then(|shooter| shooter.from.take()) else {
            return;
        };
        let rect = grab::drag_rect(from, self.pointer, self.screen);
        if !grab::big_enough(rect) {
            self.aim_shooter();
            return;
        }
        let with_bar = self.shooter.as_ref().is_some_and(|shooter| shooter.bar.is_some());
        match with_bar {
            true => {
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.chosen = Some(rect);
                }
                self.aim_shooter();
            }
            false => self.fire(Some(rect)),
        }
    }

    fn shoot(&mut self) {
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let area = match shooter.tool.pick {
            Pick::Screen => None,
            Pick::Window => match self.window_under_pointer() {
                Some(index) => {
                    self.raise(index);
                    Some(self.frame_on_screen(index))
                }
                None => return,
            },
            Pick::Portion => match shooter.chosen {
                Some(rect) => Some(rect),
                None => return,
            },
        };
        self.fire(area);
    }

    fn fire(&mut self, area: Option<Rect>) {
        let Some(shooter) = self.shooter.take() else {
            return;
        };
        self.popover = None;
        self.shot_tool = shooter.tool;
        match shooter.tool.take {
            Take::Still => self.take_still(area),
            Take::Video => self.start_recording(area),
        }
    }

    pub(super) fn take_still(&mut self, area: Option<Rect>) {
        self.shot_crop = area;
        self.shot_wanted = true;
        self.kick_now();
    }

    fn shooter_options(&mut self, bar: Rect) {
        self.background("sound-sources", move || {
            let listing = std::process::Command::new("pactl")
                .args(["list", "sources"])
                .env("LC_ALL", "C")
                .stderr(std::process::Stdio::null())
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
                .unwrap_or_default();
            move |state: &mut State| {
                if state.shooter.is_some() {
                    state.shooter_sources(bar, &listing);
                }
            }
        });
    }

    fn shooter_sources(&mut self, bar: Rect, listing: &str) {
        let sources = grab::sources(listing);
        let mark = |on: bool, label: &str| match on {
            true => format!("• {label}"),
            false => format!("   {label}"),
        };
        let mut items = vec![drifter_core::menu::Item {
            enabled: false,
            ..drifter_core::menu::Item::line(-10, drifter_core::lang::t("Sound"))
        }];
        items.push(drifter_core::menu::Item::line(0, &mark(self.sound_from.is_none(), drifter_core::lang::t("None"))));
        for (at, source) in sources.iter().enumerate() {
            let on = self.sound_from.as_deref() == Some(source.name.as_str());
            items.push(drifter_core::menu::Item::line(at as i32 + 1, &mark(on, &source.label)));
        }
        if let Some(shooter) = self.shooter.as_mut() {
            shooter.sources = sources;
        }
        let look = self.bar_look();
        let options = look
            .slots()
            .into_iter()
            .find(|(_, slot)| *slot == Slot::Options)
            .map(|(rect, _)| rect)
            .unwrap_or(Rect::new(Vec2::ZERO, bar.size));
        let size = self.menu_look().size(&items, self.font.as_deref());
        let at = Vec2::new(bar.origin.x + options.origin.x, bar.origin.y - size.y - 8.0);
        self.open_popover(items, at, Target::Shooter);
    }

    pub(super) fn shooter_menu_pick(&mut self, id: i32) {
        self.popover = None;
        let chosen = match id {
            0 => None,
            id => self
                .shooter
                .as_ref()
                .and_then(|shooter| shooter.sources.get(id as usize - 1))
                .map(|source| source.name.clone()),
        };
        if id >= 0 {
            self.sound_from = chosen;
        }
        self.kick_now();
    }

    fn start_recording(&mut self, area: Option<Rect>) {
        if self.recording.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = self.shots.folder_in(&home);
        let stamp = drifter_core::tray::now("%Y-%m-%d at %H.%M.%S").unwrap_or_default();
        let path = folder.join(grab::file_name(Take::Video, &stamp, "mp4"));
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.recording_done(Err(error.to_string()));
            return;
        }
        let Some(head) = self.heads.get(self.hand) else {
            return;
        };
        let area = area.map(|rect| Rect::new(rect.origin + head.place, rect.size));
        let args = grab::recorder_args(&head.name, area, self.sound_from.as_deref(), &path);
        let spawned = crate::apps::fresh_signals(&mut std::process::Command::new("wf-recorder"))
            .args(&args)
            .env("WAYLAND_DISPLAY", &self.socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let child = match spawned {
            Ok(child) => child,
            Err(error) => {
                self.recording_done(Err(format!("screen recording needs wf-recorder: {error}")));
                return;
            }
        };
        self.recording = Some(Recording {
            child,
            path,
            began: Instant::now(),
            art: None,
            shown: u64::MAX,
            stopping: false,
            lane: worker::Lane::default(),
        });
        self.paint_stop();
        let _ = self.handle.insert_source(
            Timer::from_duration(Duration::from_millis(250)),
            |_, _, state: &mut State| match state.tick_recording() {
                true => TimeoutAction::ToDuration(Duration::from_millis(250)),
                false => TimeoutAction::Drop,
            },
        );
        self.kick_now();
    }

    fn paint_stop(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(recording) = self.recording.as_ref() else {
            return;
        };
        let seconds = recording.began.elapsed().as_secs();
        if seconds == recording.shown {
            return;
        }
        let screen = self.screen;
        self.paint_in_lane_sized(
            "recording",
            |state| state.recording.as_mut().map(|recording| &mut recording.lane),
            State::paint_stop,
            move || Some(grab::paint_stop(seconds, &font)),
            move |state, art, size| {
                let rect = Rect::new(Vec2::new(((screen.x - size.x) / 2.0).round(), 4.0), size);
                if let Some(recording) = state.recording.as_mut() {
                    recording.art = Some((art, rect));
                    recording.shown = seconds;
                }
            },
        );
    }

    fn tick_recording(&mut self) -> bool {
        let Some(recording) = self.recording.as_mut() else {
            return false;
        };
        match recording.child.try_wait() {
            Ok(None) => {
                if recording.stopping {
                    self.unblock_copies();
                } else {
                    self.paint_stop();
                }
                true
            }
            Ok(Some(_)) | Err(_) => {
                let path = recording.path.clone();
                self.recording = None;
                let saved = std::fs::metadata(&path)
                    .ok()
                    .filter(|meta| meta.len() > 0)
                    .map(|_| path)
                    .ok_or_else(|| "the recorder stopped before it saved anything".to_string());
                self.recording_done(saved);
                self.kick_now();
                false
            }
        }
    }

    fn unblock_copies(&mut self) {
        for copy in &mut self.copies {
            copy.damage = false;
        }
        self.kick_now();
    }

    pub(super) fn stop_recording(&mut self) {
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        if !recording.stopping {
            recording.stopping = true;
            unsafe { libc::kill(recording.child.id() as i32, libc::SIGINT) };
        }
        recording.art = None;
        self.unblock_copies();
    }

    fn recording_done(&mut self, saved: Result<PathBuf, String>) {
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let (summary, body) = match saved {
            Ok(path) => {
                if let Some(folder) = path.parent() {
                    self.note_folders.insert(id, folder.to_path_buf());
                }
                (drifter_core::lang::t("Screen recording saved").to_string(), path.display().to_string())
            }
            Err(error) => (drifter_core::lang::t("No screen recording").to_string(), error),
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Drifter").to_string(),
            summary,
            body,
            actions: Vec::new(),
            expire: Some(drifter_core::notify::DEFAULT_MS),
            asked: -1,
            critical: false,
            icon: None,
        });
    }

    pub(super) fn show_thumb(&mut self, shot: &drifter_core::Image) {
        let image = grab::paint_thumb(shot, THUMB_WIDE);
        let rect = grab::thumb_rect(drifter_core::paint::logical(&image), self.screen);
        let Ok(Some(art)) = upload(&mut self.renderer, Some(&image), "thumbnail") else {
            return;
        };
        self.thumb = Some(Thumb {
            art,
            rect,
            path: None,
            born: Instant::now(),
            leaving: None,
            pressed: None,
        });
    }

    pub(super) fn wind_the_thumb(&mut self) {
        let pointer = self.pointer;
        let menu_open = matches!(self.popover.as_ref().map(|open| &open.target), Some(Target::Shot));
        let Some(thumb) = self.thumb.as_mut() else {
            return;
        };
        if let Some(since) = thumb.leaving {
            if since.elapsed().as_secs_f64() >= THUMB_OUT {
                self.thumb = None;
            }
            return;
        }
        let held = thumb.rect.contains(pointer) || thumb.pressed.is_some() || menu_open;
        if held {
            thumb.born = Instant::now() - Duration::from_secs_f64(THUMB_IN);
        } else if thumb.born.elapsed() >= THUMB_TIME {
            thumb.leaving = Some(Instant::now());
        }
    }

    pub(super) fn release_thumb(&mut self) {
        let Some(thumb) = self.thumb.as_mut() else {
            return;
        };
        if thumb.pressed.take().is_some()
            && thumb.rect.contains(self.pointer)
            && let Some(path) = thumb.path.clone()
        {
            thumb.leaving = Some(Instant::now());
            crate::apps::launch(&["xdg-open".to_string(), path.display().to_string()], &self.socket);
        }
    }

    pub(super) fn drag_thumb(&mut self) {
        use smithay::input::pointer::GrabStartData;
        use smithay::utils::SERIAL_COUNTER;
        use smithay::wayland::selection::data_device::{SourceMetadata, start_dnd};
        use smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction;

        let pointer = self.pointer;
        let Some(thumb) = self.thumb.as_mut() else {
            return;
        };
        let (Some(from), Some(path)) = (thumb.pressed, thumb.path.clone()) else {
            return;
        };
        if (pointer - from).length() < 6.0 {
            return;
        }
        thumb.pressed = None;
        thumb.leaving = Some(Instant::now());
        self.drag_shot = Some(path.clone());
        let world = self.in_world(pointer);
        let metadata = SourceMetadata {
            mime_types: vec!["text/uri-list".to_string(), shot_mime(&path).to_string()],
            dnd_action: DndAction::Copy,
        };
        let start = GrabStartData {
            focus: None,
            button: BTN_LEFT,
            location: (world.x, world.y).into(),
        };
        let dh = self.wayland.handle.clone();
        let seat = self.wayland._seat.clone();
        start_dnd(&dh, &seat, self, SERIAL_COUNTER.next_serial(), Some(start), None, metadata);
    }

    pub(super) fn shot_menu_pick(&mut self, id: i32) {
        self.popover = None;
        let Some(path) = self.thumb.as_ref().and_then(|thumb| thumb.path.clone()) else {
            return;
        };
        match id {
            SHOT_COPY => {
                if let Ok(bytes) = std::fs::read(&path) {
                    let mime = shot_mime(&path).to_string();
                    let dh = self.wayland.handle.clone();
                    let seat = self.wayland._seat.clone();
                    smithay::wayland::selection::data_device::set_data_device_selection(
                        &dh,
                        &seat,
                        vec![mime.clone()],
                        std::sync::Arc::new(vec![(mime, bytes)]),
                    );
                }
            }
            SHOT_FOLDER => {
                if let Some(folder) = path.parent() {
                    crate::apps::open_folder(&folder.display().to_string(), self.apps_chosen.get(drifter_core::desktop::Role::Files), &self.socket);
                }
            }
            SHOT_OPEN => {
                crate::apps::launch(&["xdg-open".to_string(), path.display().to_string()], &self.socket);
            }
            SHOT_DELETE => {
                let _ = std::fs::remove_file(&path);
            }
            _ => {}
        }
        if let Some(thumb) = self.thumb.as_mut() {
            thumb.leaving = Some(Instant::now());
        }
        self.kick_now();
    }

    pub(super) fn press_thumb(&mut self, code: u32) -> bool {
        let on = self
            .thumb
            .as_ref()
            .is_some_and(|thumb| thumb.rect.contains(self.pointer));
        if !on {
            return false;
        }
        let pointer = self.pointer;
        if code == BTN_LEFT
            && let Some(thumb) = self.thumb.as_mut()
        {
            thumb.pressed = Some(pointer);
        }
        if code == BTN_RIGHT {
            let ready = self.thumb.as_ref().is_some_and(|thumb| thumb.path.is_some());
            let item = |id: i32, label: &str| drifter_core::menu::Item {
                enabled: ready,
                ..drifter_core::menu::Item::line(id, label)
            };
            let items = vec![
                item(SHOT_COPY, drifter_core::lang::t("Copy")),
                item(SHOT_FOLDER, drifter_core::lang::t("Show in Folder")),
                item(SHOT_OPEN, drifter_core::lang::t("Open")),
                drifter_core::menu::Item::rule(90),
                item(SHOT_DELETE, "Delete"),
            ];
            self.open_popover(items, pointer, Target::Shot);
        }
        self.kick_now();
        true
    }

    pub(super) fn press_stop(&mut self, code: u32) -> bool {
        let on = self
            .recording
            .as_ref()
            .and_then(|recording| recording.art.as_ref())
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        if on && code == BTN_LEFT {
            self.stop_recording();
        }
        on
    }

    pub(super) fn own_elements(&self, elements: &mut Vec<Skin>) {
        if let Some((art, rect)) = self.recording.as_ref().and_then(|recording| recording.art.as_ref()) {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }
        if let Some(thumb) = self.thumb.as_ref() {
            let shown = (thumb.born.elapsed().as_secs_f64() / THUMB_IN).min(1.0);
            let out = thumb
                .leaving
                .map_or(0.0, |since| drifter_core::motion::smooth(since.elapsed().as_secs_f64() / THUMB_OUT));
            let slide = out * (self.screen.x - thumb.rect.origin.x + 20.0);
            let rect = Rect::new(thumb.rect.origin + Vec2::new(slide, 0.0), thumb.rect.size);
            elements.push(Skin::Texture(element(&thumb.art, rect, (shown * (1.0 - out * 0.4)) as f32)));
        }
    }

    pub(super) fn thumb_moving(&self) -> bool {
        self.thumb
            .as_ref()
            .is_some_and(|thumb| thumb.born.elapsed() < Duration::from_millis(300) || thumb.leaving.is_some())
    }

    pub(super) fn shooter_cursor(&self) -> Option<Vec<Skin>> {
        let shooter = self.shooter.as_ref()?;
        let on_bar = shooter
            .bar
            .as_ref()
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        if on_bar || shooter.tool.pick != Pick::Portion {
            return None;
        }
        let (_, inks) = self.shot_inks.as_ref()?;
        let mut elements = Vec::new();
        if let Some((art, _, size)) = shooter.tag.as_ref() {
            let size = *size;
            let at = self.pointer + Vec2::new(14.0, 14.0);
            let at = at.min(self.screen - size - Vec2::splat(4.0));
            elements.push(Skin::Texture(element(art, Rect::new(at, size), 1.0)));
        }
        let size = Vec2::splat(CROSS as f64);
        let rect = Rect::new(self.pointer - (size / 2.0).round(), size);
        elements.push(Skin::Texture(element(&inks.cross, rect, 1.0)));
        Some(elements)
    }

    pub(super) fn shooter_elements(&self, elements: &mut Vec<Skin>) {
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let Some((_, inks)) = self.shot_inks.as_ref() else {
            return;
        };
        if let Some(open) = self.popover.as_ref()
            && let Some(art) = open.art.as_ref()
        {
            elements.push(Skin::Texture(element(art, open.rect, 1.0)));
        }
        if let Some((art, rect)) = shooter.bar.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
            self.modal_shadow(*rect, elements);
        }
        let boxed = shooter
            .from
            .map(|from| grab::drag_rect(from, self.pointer, self.screen))
            .or(shooter.chosen);
        if let Some(rect) = boxed {
            let thick = 1.0;
            let (o, s) = (rect.origin, rect.size);
            for (side, ink) in [
                Rect::new(o, Vec2::new(s.x, thick)),
                Rect::new(Vec2::new(o.x, o.y + s.y - thick), Vec2::new(s.x, thick)),
                Rect::new(o, Vec2::new(thick, s.y)),
                Rect::new(Vec2::new(o.x + s.x - thick, o.y), Vec2::new(thick, s.y)),
            ]
            .into_iter()
            .zip(&inks.sides)
            {
                elements.push(Skin::Texture(fill(ink, side, 0.9)));
            }
            let screen = self.screen;
            let max = rect.max();
            for (shade, ink) in [
                Rect::new(Vec2::ZERO, Vec2::new(screen.x, o.y)),
                Rect::new(Vec2::new(0.0, max.y), Vec2::new(screen.x, screen.y - max.y)),
                Rect::new(Vec2::new(0.0, o.y), Vec2::new(o.x, s.y)),
                Rect::new(Vec2::new(max.x, o.y), Vec2::new(screen.x - max.x, s.y)),
            ]
            .into_iter()
            .zip(&inks.shades)
            {
                if shade.size.x > 0.0 && shade.size.y > 0.0 {
                    elements.push(Skin::Texture(fill(ink, shade, 0.35)));
                }
            }
        }
        if let Some(rect) = shooter.under {
            elements.push(Skin::Texture(fill(&inks.accent, rect, 0.3)));
        }
    }
}

fn shot_mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "image/png",
    }
}

pub fn send_shot(path: &std::path::Path, mime: &str, fd: std::os::fd::OwnedFd) {
    use std::io::Write as _;
    let path = path.to_path_buf();
    let mime = mime.to_string();
    std::thread::spawn(move || {
        let bytes = match mime.as_str() {
            "text/uri-list" => format!("file://{}\r\n", path.display()).into_bytes(),
            _ => std::fs::read(&path).unwrap_or_default(),
        };
        let _ = std::fs::File::from(fd).write_all(&bytes);
    });
}
