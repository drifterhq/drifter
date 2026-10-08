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
            let chosen = self
                .monitors
                .iter()
                .find(|each| each.name == name)
                .and_then(|each| each.profile.clone())
                .filter(|path| !path.trim().is_empty());
            let key = Some((wanted, chosen.clone()));
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

struct Stages {
    degamma: Option<(smithay::reexports::drm::control::property::Handle, usize)>,
    ctm: Option<smithay::reexports::drm::control::property::Handle>,
    gamma: Option<(smithay::reexports::drm::control::property::Handle, usize)>,
}

fn stages(drm: &DrmDevice, crtc: crtc::Handle) -> Stages {
    use smithay::reexports::drm::control::Device as _;

    let mut found = std::collections::HashMap::new();
    if let Ok(props) = drm.get_properties(crtc) {
        for (handle, value) in props.iter() {
            if let Ok(info) = drm.get_property(*handle) {
                found.insert(info.name().to_string_lossy().into_owned(), (*handle, *value));
            }
        }
    }
    let sized = |lut: &str, size: &str| -> Option<(smithay::reexports::drm::control::property::Handle, usize)> {
        let (handle, _) = found.get(lut)?;
        let (_, length) = found.get(size)?;
        (*length > 1).then_some((*handle, *length as usize))
    };
    Stages {
        degamma: sized("DEGAMMA_LUT", "DEGAMMA_LUT_SIZE"),
        ctm: found.get("CTM").map(|(handle, _)| *handle),
        gamma: sized("GAMMA_LUT", "GAMMA_LUT_SIZE"),
    }
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
    crtc: crtc::Handle,
    prop: smithay::reexports::drm::control::property::Handle,
    bytes: Option<&mut [u8]>,
) -> Result<(), String> {
    use smithay::reexports::drm::control::Device as _;

    let id = match bytes {
        Some(bytes) => blob(drm, bytes)?,
        None => 0,
    };
    let set = drm.set_property(crtc, prop, id).map_err(|error| format!("the screen refused the colors: {error}"));
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
