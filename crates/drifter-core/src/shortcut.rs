use crate::config::Modifier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Switch,
    Close,
    Screenshot,
    Lock,
    Power,
    Overview,
    Terminal,
    Files,
    Browser,
    Settings,
}

impl Action {
    pub const ALL: [Action; 10] = [
        Action::Switch,
        Action::Close,
        Action::Screenshot,
        Action::Lock,
        Action::Power,
        Action::Overview,
        Action::Terminal,
        Action::Files,
        Action::Browser,
        Action::Settings,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Action::Switch => "switch",
            Action::Close => "close",
            Action::Screenshot => "screenshot",
            Action::Lock => "lock",
            Action::Power => "power",
            Action::Overview => "overview",
            Action::Terminal => "terminal",
            Action::Files => "files",
            Action::Browser => "browser",
            Action::Settings => "settings",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Switch => "Switch windows",
            Action::Close => "Close the window",
            Action::Screenshot => "Take a screenshot",
            Action::Lock => "Lock the screen",
            Action::Power => "Power menu",
            Action::Overview => "Overview",
            Action::Terminal => "Open the terminal",
            Action::Files => "Open the file manager",
            Action::Browser => "Open the web browser",
            Action::Settings => "Open Settings",
        }
    }

    pub fn default_combo(self) -> &'static str {
        match self {
            Action::Switch => "alt+Tab",
            Action::Close => "alt+F4",
            Action::Screenshot => "Print",
            Action::Lock => "camera+l",
            Action::Power => "camera+Escape",
            Action::Overview => "",
            Action::Terminal => "",
            Action::Files => "",
            Action::Browser => "",
            Action::Settings => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Combo {
    pub mods: Mods,
    pub camera: bool,
    pub key: String,
}

const ALONE: [&str; 6] = ["Print", "Pause", "Scroll_Lock", "Menu", "Insert", "Help"];

pub fn may_stand_alone(key: &str) -> bool {
    let function = key
        .strip_prefix(['F', 'f'])
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));
    function || key.starts_with("XF86") || ALONE.iter().any(|name| name.eq_ignore_ascii_case(key))
}

pub fn is_modifier(key: &str) -> bool {
    let key = key.to_lowercase();
    [
        "shift_l",
        "shift_r",
        "control_l",
        "control_r",
        "alt_l",
        "alt_r",
        "super_l",
        "super_r",
        "meta_l",
        "meta_r",
        "hyper_l",
        "hyper_r",
        "iso_level3_shift",
        "iso_level5_shift",
        "caps_lock",
        "num_lock",
    ]
    .contains(&key.as_str())
}

impl Combo {
    pub fn parse(text: &str) -> Result<Option<Combo>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let (key, mods) = match parts.split_last() {
            Some((key, mods)) if !key.is_empty() => (key, mods),
            _ => return Err(format!("shortcut {text:?} has no key after the last +")),
        };
        let mut combo = Combo {
            key: key.to_string(),
            ..Combo::default()
        };
        for word in mods {
            match word.to_lowercase().as_str() {
                "ctrl" | "control" => combo.mods.ctrl = true,
                "alt" => combo.mods.alt = true,
                "shift" => combo.mods.shift = true,
                "super" | "logo" | "win" => combo.mods.logo = true,
                "camera" => combo.camera = true,
                other => {
                    return Err(format!(
                        "shortcut {text:?}: {other:?} is not ctrl, alt, shift, super or camera"
                    ));
                }
            }
        }
        Ok(Some(combo))
    }

    pub fn word(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.camera {
            parts.push("camera");
        }
        if self.mods.ctrl {
            parts.push("ctrl");
        }
        if self.mods.alt {
            parts.push("alt");
        }
        if self.mods.shift {
            parts.push("shift");
        }
        if self.mods.logo {
            parts.push("super");
        }
        parts.push(&self.key);
        parts.join("+")
    }

    pub fn resolve(&self, camera: Modifier) -> Combo {
        let mut mods = self.mods;
        if self.camera {
            match camera {
                Modifier::Super => mods.logo = true,
                Modifier::Alt => mods.alt = true,
                Modifier::Ctrl => mods.ctrl = true,
            }
        }
        Combo {
            mods,
            camera: false,
            key: self.key.clone(),
        }
    }

    pub fn shown(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.camera {
            parts.push("Camera key".into());
        }
        let mods = [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
            (self.mods.logo, "Super"),
        ];
        for (held, name) in mods {
            if held {
                parts.push(name.into());
            }
        }
        parts.push(pretty(&self.key));
        parts.join(" + ")
    }

    pub fn matches(&self, held: Mods, names: &[String], ignore_shift: bool) -> bool {
        let shift_fits = ignore_shift || held.shift == self.mods.shift;
        held.ctrl == self.mods.ctrl
            && held.alt == self.mods.alt
            && held.logo == self.mods.logo
            && shift_fits
            && names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&self.key))
    }
}

pub fn keysym_word(name: &str) -> String {
    match name.strip_prefix("XF86XK_") {
        Some(rest) => format!("XF86{rest}"),
        None => name.strip_prefix("XK_").unwrap_or(name).to_string(),
    }
}

pub fn pretty(key: &str) -> String {
    match key {
        "Return" => "Enter".into(),
        "Escape" => "Esc".into(),
        "Print" => "Print Screen".into(),
        "space" => "Space".into(),
        "BackSpace" => "Backspace".into(),
        "Prior" => "Page Up".into(),
        "Next" => "Page Down".into(),
        key if key.chars().count() == 1 => key.to_uppercase(),
        key => key.replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_parses_and_round_trips() {
        for action in Action::ALL {
            let text = action.default_combo();
            match Combo::parse(text).unwrap() {
                Some(combo) => assert_eq!(combo.word(), text),
                None => assert!(text.is_empty()),
            }
        }
    }

    #[test]
    fn a_bad_shortcut_says_why() {
        assert!(Combo::parse("hyperdrive+x").is_err());
        assert!(Combo::parse("ctrl+").is_err());
        assert_eq!(Combo::parse("  ").unwrap(), None);
    }

    #[test]
    fn the_camera_key_becomes_its_modifier() {
        let lock = Combo::parse("camera+l").unwrap().unwrap();
        assert_eq!(lock.shown(), "Camera key + L");
        let plain = lock.resolve(Modifier::Ctrl);
        assert!(plain.mods.ctrl && !plain.camera);
        let held = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert!(plain.matches(held, &["L".into()], false));
        assert!(!plain.matches(Mods::default(), &["l".into()], false));
    }

    #[test]
    fn switching_backwards_keeps_shift_free() {
        let switch = Combo::parse("alt+Tab").unwrap().unwrap();
        let held = Mods {
            alt: true,
            shift: true,
            ..Mods::default()
        };
        let names = ["ISO_Left_Tab".to_string(), "Tab".to_string()];
        assert!(switch.matches(held, &names, true));
        assert!(!switch.matches(held, &names, false));
    }

    #[test]
    fn only_some_keys_work_without_a_modifier() {
        assert!(may_stand_alone("F5") && may_stand_alone("Print"));
        assert!(may_stand_alone("XF86Calculator"));
        assert!(!may_stand_alone("a") && !may_stand_alone("Return") && !may_stand_alone("F"));
        assert!(is_modifier("Shift_L") && !is_modifier("a"));
    }

    #[test]
    fn keysym_names_lose_their_prefix() {
        assert_eq!(keysym_word("XK_Tab"), "Tab");
        assert_eq!(keysym_word("XF86XK_AudioMute"), "XF86AudioMute");
    }

    #[test]
    fn keys_read_well() {
        let combo = Combo::parse("ctrl+shift+Return").unwrap().unwrap();
        assert_eq!(combo.shown(), "Ctrl + Shift + Enter");
    }
}
