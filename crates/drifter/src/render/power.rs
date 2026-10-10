use super::*;

impl State {
    pub(super) fn watch_night(&mut self) {
        let Some(drm) = self.drm.as_ref().filter(|_| self.active) else {
            return;
        };
        let night = self.night.on
            && drifter_core::night::is_night(
                self.night.start,
                self.night.end,
                drifter_core::night::hour_now(),
            );
        let wanted = match night {
            true => self.night.warmth,
            false => drifter_core::night::PLAIN,
        };
        let white = drifter_core::night::white_point(wanted);
        for index in 0..self.heads.len() {
            let name = self.heads[index].name.clone();
            let own = self.monitors.iter().find(|each| each.name == name);
            let chosen = own.and_then(|each| each.profile.clone()).filter(|path| !path.trim().is_empty());
            let hdr = own
                .filter(|each| each.hdr)
                .map(|each| each.hdr_white.unwrap_or(drifter_core::hdr::WHITE));
            let key = Some((wanted, chosen.clone(), hdr));
            if self.heads[index].warmth == key {
                continue;
            }
            self.heads[index].warmth = key;
            let profile = chosen.as_deref().and_then(|path| {
                drifter_core::color::load(std::path::Path::new(path))
                    .inspect_err(|error| log!("drifter: the color profile for {name} was not used: {error}"))
                    .ok()
            });
            let crtc = self.heads[index].crtc;
            let connector = self.heads[index].connector;
            let screen = hdr.and_then(|_| drifter_core::hdr::screen(&name));
            if hdr.is_some() && screen.is_none() {
                log!("drifter: {name} does not say it can show HDR, so it stays SDR");
            }
            if let (Some(sdr), Some(screen)) = (hdr, screen) {
                match paint_hdr(drm, crtc, connector, screen, white, sdr) {
                    Ok(()) => {
                        log!("drifter: {name} shows HDR, with white at {sdr} nits");
                        continue;
                    }
                    Err(error) => log!("drifter: {name} cannot show HDR: {error}"),
                }
            }
            if let Err(error) = plain_signal(drm, crtc, connector) {
                log!("drifter: {name} cannot leave HDR: {error}");
            }
            if let Err(error) = paint_color(drm, crtc, profile.as_ref(), white, night) {
                log!("drifter: colors on {name}: {error}");
            }
        }
    }

    pub(super) fn watch_idle(&mut self) {
        if !self.wayland.extras.inhibitors.is_empty() {
            self.last_input = Instant::now();
            return;
        }
        let idle = self.last_input.elapsed();
        if self.lock_after > 0
            && self.lock.is_none()
            && idle >= Duration::from_secs(self.lock_after.saturating_mul(60))
        {
            self.lock_now();
        }
        if self.screen_off > 0
            && !self.dark
            && idle >= Duration::from_secs(self.screen_off.saturating_mul(60))
        {
            self.go_dark();
        }
        let sleep = self.power_rules.sleep_after;
        if sleep > 0 && !self.slept && idle >= Duration::from_secs(sleep.saturating_mul(60)) {
            self.slept = true;
            self.lock_now();
            crate::power::run(drifter_core::power::Action::Suspend);
        }
    }

    pub(super) fn lid_closed(&mut self) {
        use drifter_core::Lid;
        match self.power_rules.lid {
            Lid::Nothing => {}
            Lid::Lock => {
                self.lock_now();
            }
            Lid::Suspend => {
                self.lock_now();
                if self.heads.len() <= 1 {
                    crate::power::run(drifter_core::power::Action::Suspend);
                }
            }
        }
    }

    pub(super) fn power_pressed(&mut self) {
        use drifter_core::PowerKey;
        use drifter_core::power::Action;
        match self.power_rules.button {
            PowerKey::Nothing => {}
            PowerKey::Menu => {
                if self.lock.is_none() {
                    let middle = self.screen * 0.5;
                    self.open_popover(drifter_core::power::items(), middle, Target::Desktop);
                }
            }
            PowerKey::Suspend => {
                self.lock_now();
                crate::power::run(Action::Suspend);
            }
            PowerKey::Off => {
                if drifter_core::reopen::wanted() {
                    self.save_desk();
                }
                crate::power::run(Action::Shutdown);
            }
        }
    }

    pub(super) fn go_dark(&mut self) {
        self.dark = true;
        for head in &mut self.heads {
            head.awaiting = false;
            if let Err(error) = head.compositor.clear() {
                log!("drifter: cannot turn {} off: {error}", head.name);
            }
        }
    }

    pub(super) fn wake(&mut self) {
        if !self.dark {
            return;
        }
        self.dark = false;
        for head in &mut self.heads {
            head.awaiting = false;
            if let Err(error) = head.compositor.reset_state() {
                log!("drifter: cannot turn {} on: {error}", head.name);
            }
        }
        self.kick_now();
    }
}

type Prop = smithay::reexports::drm::control::property::Handle;

struct Found {
    handle: Prop,
    value: u64,
    info: smithay::reexports::drm::control::property::Info,
}

impl Found {
    fn choice(&self, name: &str) -> Option<u64> {
        use smithay::reexports::drm::control::property::ValueType;
        let ValueType::Enum(choices) = self.info.value_type() else {
            return None;
        };
        let (_, named) = choices.values();
        named.iter().find(|each| each.name().to_bytes() == name.as_bytes()).map(|each| each.value())
    }
}

fn props(
    drm: &DrmDevice,
    handle: impl smithay::reexports::drm::control::ResourceHandle,
) -> std::collections::HashMap<String, Found> {
    use smithay::reexports::drm::control::Device as _;

    let mut found = std::collections::HashMap::new();
    if let Ok(props) = drm.get_properties(handle) {
        for (prop, value) in props.iter() {
            if let Ok(info) = drm.get_property(*prop) {
                let name = info.name().to_string_lossy().into_owned();
                found.insert(name, Found { handle: *prop, value: *value, info });
            }
        }
    }
    found
}

struct Stages {
    degamma: Option<(Prop, usize)>,
    ctm: Option<Prop>,
    gamma: Option<(Prop, usize)>,
    encoder: Option<(Prop, u64, u64)>,
    divisor: Option<(Prop, u64)>,
}

fn stages(drm: &DrmDevice, crtc: crtc::Handle) -> Stages {
    let found = props(drm, crtc);
    let sized = |lut: &str, size: &str| -> Option<(Prop, usize)> {
        let lut = found.get(lut)?;
        let size = found.get(size)?;
        (size.value > 1).then_some((lut.handle, size.value as usize))
    };
    let encoder = found.get("NV_CRTC_REGAMMA_TF").and_then(|tf| {
        let pq = tf.choice("PQ (Perceptual Quantizer)")?;
        let plain = tf.choice("Default")?;
        Some((tf.handle, pq, plain))
    });
    Stages {
        degamma: sized("DEGAMMA_LUT", "DEGAMMA_LUT_SIZE"),
        ctm: found.get("CTM").map(|each| each.handle),
        gamma: sized("GAMMA_LUT", "GAMMA_LUT_SIZE"),
        encoder,
        divisor: found.get("NV_CRTC_REGAMMA_DIVISOR").map(|each| (each.handle, each.value)),
    }
}

fn paint_hdr(
    drm: &DrmDevice,
    crtc: crtc::Handle,
    connector: connector::Handle,
    screen: drifter_core::hdr::Screen,
    white: [f64; 3],
    sdr: u32,
) -> Result<(), String> {
    use smithay::reexports::drm::control::Device as _;

    let wire = props(drm, connector);
    let space = wire.get("Colorspace").ok_or("the connector has no color space setting")?;
    let bt2020 = space.choice("BT2020_RGB").ok_or("the connector cannot send BT.2020 color")?;
    let metadata = wire.get("HDR_OUTPUT_METADATA").ok_or("the connector cannot send HDR metadata")?;
    let found = stages(drm, crtc);
    let (Some((degamma, degamma_len)), Some(ctm)) = (found.degamma, found.ctm) else {
        return Err("the graphics card has no color stages for HDR".to_string());
    };
    let gamma_len = found.gamma.map_or(0, |(_, len)| len);
    let pipe = drifter_core::hdr::pipeline(white, sdr, degamma_len, gamma_len);
    set_blob(drm, crtc, degamma, Some(&mut lut_bytes(&pipe.degamma)))?;
    let mut words: Vec<u8> = drifter_core::color::ctm_words(pipe.ctm).iter().flat_map(|word| word.to_ne_bytes()).collect();
    set_blob(drm, crtc, ctm, Some(&mut words))?;
    match (found.encoder, found.divisor, found.gamma) {
        (Some((tf, pq, _)), Some((divisor, _)), gamma) => {
            if let Some((gamma, _)) = gamma {
                set_blob(drm, crtc, gamma, None)?;
            }
            let refused = |error: std::io::Error| format!("the graphics card refused PQ: {error}");
            drm.set_property(crtc, divisor, drifter_core::hdr::divisor(sdr)).map_err(refused)?;
            drm.set_property(crtc, tf, pq).map_err(refused)?;
        }
        (_, _, Some((gamma, _))) => set_blob(drm, crtc, gamma, Some(&mut lut_bytes(&pipe.gamma)))?,
        _ => return Err("the graphics card has no way to encode PQ".to_string()),
    }
    set_blob(drm, connector, metadata.handle, Some(&mut drifter_core::hdr::infoframe(screen, sdr)))?;
    if space.value != bt2020 {
        drm.set_property(connector, space.handle, bt2020)
            .map_err(|error| format!("the screen refused BT.2020 color: {error}"))?;
    }
    Ok(())
}

fn plain_signal(drm: &DrmDevice, crtc: crtc::Handle, connector: connector::Handle) -> Result<(), String> {
    use smithay::reexports::drm::control::Device as _;

    let refused = |error: std::io::Error| format!("the screen refused SDR: {error}");
    let wire = props(drm, connector);
    if let Some(metadata) = wire.get("HDR_OUTPUT_METADATA").filter(|each| each.value != 0) {
        set_blob(drm, connector, metadata.handle, None)?;
    }
    if let Some(space) = wire.get("Colorspace")
        && let Some(plain) = space.choice("Default")
        && space.value != plain
    {
        drm.set_property(connector, space.handle, plain).map_err(refused)?;
    }
    let found = stages(drm, crtc);
    let crtc_props = props(drm, crtc);
    if let Some((tf, _, plain)) = found.encoder
        && crtc_props.get("NV_CRTC_REGAMMA_TF").is_some_and(|each| each.value != plain)
    {
        drm.set_property(crtc, tf, plain).map_err(refused)?;
    }
    if let Some((divisor, now)) = found.divisor
        && now != 1 << 32
    {
        drm.set_property(crtc, divisor, 1 << 32).map_err(refused)?;
    }
    Ok(())
}

fn blob(drm: &DrmDevice, bytes: &mut [u8]) -> Result<u64, String> {
    use std::os::fd::AsFd;
    drm_ffi::mode::create_property_blob(drm.as_fd(), bytes)
        .map(|made| u64::from(made.blob_id))
        .map_err(|error| format!("cannot hand the colors to the screen: {error}"))
}

fn lut_bytes(entries: &[[u16; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 8);
    for [red, green, blue] in entries {
        for word in [*red, *green, *blue, 0] {
            out.extend(word.to_ne_bytes());
        }
    }
    out
}

fn set_blob(
    drm: &DrmDevice,
    handle: impl smithay::reexports::drm::control::ResourceHandle,
    prop: Prop,
    bytes: Option<&mut [u8]>,
) -> Result<(), String> {
    use smithay::reexports::drm::control::Device as _;

    let id = match bytes {
        Some(bytes) => blob(drm, bytes)?,
        None => 0,
    };
    let set = drm.set_property(handle, prop, id).map_err(|error| format!("the screen refused the colors: {error}"));
    if id != 0 {
        let _ = drm.destroy_property_blob(id);
    }
    set
}

fn paint_color(
    drm: &DrmDevice,
    crtc: crtc::Handle,
    profile: Option<&drifter_core::color::Profile>,
    white: [f64; 3],
    night: bool,
) -> Result<(), String> {
    use smithay::reexports::drm::control::Device as _;

    let found = stages(drm, crtc);
    if let (Some(profile), Some((degamma, degamma_len)), Some(ctm), Some((gamma, gamma_len))) =
        (profile, found.degamma, found.ctm, found.gamma)
        && let Some(pipe) = drifter_core::color::pipeline(profile, white, degamma_len, gamma_len)
    {
        set_blob(drm, crtc, degamma, Some(&mut lut_bytes(&pipe.degamma)))?;
        let mut words: Vec<u8> = drifter_core::color::ctm_words(pipe.ctm).iter().flat_map(|word| word.to_ne_bytes()).collect();
        set_blob(drm, crtc, ctm, Some(&mut words))?;
        set_blob(drm, crtc, gamma, Some(&mut lut_bytes(&pipe.gamma)))?;
        return Ok(());
    }
    if let Some((degamma, _)) = found.degamma {
        let _ = set_blob(drm, crtc, degamma, None);
    }
    if let Some(ctm) = found.ctm {
        let _ = set_blob(drm, crtc, ctm, None);
    }
    let length = drm
        .get_crtc(crtc)
        .map(|info| info.gamma_length() as usize)
        .map_err(|error| error.to_string())?;
    if length == 0 {
        return match night || profile.is_some() {
            true => Err("this screen has no gamma ramp, so no night light or color profile".to_string()),
            false => Ok(()),
        };
    }
    let [red, green, blue] = drifter_core::color::calibration(profile, white, length);
    drm.set_gamma(crtc, &red, &green, &blue).map_err(|error| error.to_string())
}
