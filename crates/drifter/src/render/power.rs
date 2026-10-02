use super::*;

impl State {
    pub(super) fn watch_night(&mut self) {
        use smithay::reexports::drm::control::Device as _;

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
        for head in &mut self.heads {
            if head.warmth == Some(wanted) {
                continue;
            }
            head.warmth = Some(wanted);
            let length = match drm.get_crtc(head.crtc) {
                Ok(info) => info.gamma_length() as usize,
                Err(error) => {
                    eprintln!("drifter: no night light on {}: {error}", head.name);
                    continue;
                }
            };
            if length == 0 {
                if night {
                    eprintln!(
                        "drifter: {} has no gamma ramp, so no night light",
                        head.name
                    );
                }
                continue;
            }
            let [red, green, blue] = drifter_core::night::ramp(length, wanted);
            if let Err(error) = drm.set_gamma(head.crtc, &red, &green, &blue) {
                eprintln!("drifter: no night light on {}: {error}", head.name);
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
            PowerKey::Off => crate::power::run(Action::Shutdown),
        }
    }

    pub(super) fn go_dark(&mut self) {
        self.dark = true;
        for head in &mut self.heads {
            head.awaiting = false;
            if let Err(error) = head.compositor.clear() {
                eprintln!("drifter: cannot turn {} off: {error}", head.name);
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
                eprintln!("drifter: cannot turn {} on: {error}", head.name);
            }
        }
        self.kick_now();
    }
}
