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
    Search,
    ShotScreen,
    ShotPart,
    ShotBar,
    ScreenReader,
}

impl Action {
    pub const ALL: [Action; 15] = [
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
        Action::Search,
        Action::ShotScreen,
        Action::ShotPart,
        Action::ShotBar,
        Action::ScreenReader,
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
            Action::Search => "search",
            Action::ShotScreen => "shot_screen",
            Action::ShotPart => "shot_part",
            Action::ShotBar => "shot_bar",
            Action::ScreenReader => "screen_reader",
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
            Action::Search => "Search",
            Action::ShotScreen => "Screenshot of the screen",
            Action::ShotPart => "Screenshot of a part or a window",
            Action::ShotBar => "Screenshot and recording bar",
            Action::ScreenReader => "Turn the screen reader on or off",
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
            Action::Search => "super+space",
            Action::ShotScreen => "shift+super+3",
            Action::ShotPart => "shift+super+4",
            Action::ShotBar => "shift+super+5",
            Action::ScreenReader => "alt+super+s",
        }
    }
}

const BUILT_IN: [(&str, Action); 2] = [("XF86Search", Action::Search), ("XF86LaunchA", Action::Overview)];

pub fn built_in(held: Mods, names: &[String]) -> Option<Action> {
    if held != Mods::default() {
        return None;
    }
    BUILT_IN
        .iter()
        .find(|(key, _)| names.iter().any(|name| name.eq_ignore_ascii_case(key)))
        .map(|(_, action)| *action)
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
            let held = match camera {
                Modifier::Super => &mut mods.logo,
                Modifier::Alt => &mut mods.alt,
                Modifier::Ctrl => &mut mods.ctrl,
            };
            match *held {
                true => mods.shift = true,
                false => *held = true,
            }
        }
        Combo {
            mods,
            camera: false,
            key: self.key.clone(),
        }
    }

    pub fn shown(&self, camera: crate::config::Modifier) -> String {
        let real = self.resolve(camera);
        let mut parts: Vec<String> = Vec::new();
        let mods = [
            (real.mods.ctrl, "Ctrl"),
            (real.mods.alt, "Alt"),
            (real.mods.shift, "Shift"),
            (real.mods.logo, "Super"),
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

const FIRST_VT: u32 = 0x1008_fe01;
const LAST_VT: u32 = 0x1008_fe0c;

pub fn is_modifier_code(code: u32) -> bool {
    const EVDEV: [u32; 8] = [29, 97, 42, 54, 56, 100, 125, 126];
    EVDEV.iter().any(|evdev| evdev + 8 == code)
}

pub fn vt_number(keysym: u32) -> Option<i32> {
    (FIRST_VT..=LAST_VT)
        .contains(&keysym)
        .then(|| (keysym - FIRST_VT + 1) as i32)
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
    #[test]
    fn saving_a_place_never_takes_the_fly_back_key() {
        use crate::config::Modifier;
        let go = Combo::parse("camera+1").unwrap().unwrap();
        let save = Combo::parse("camera+ctrl+1").unwrap().unwrap();
        for camera in [Modifier::Super, Modifier::Alt, Modifier::Ctrl] {
            assert_ne!(go.resolve(camera), save.resolve(camera), "{camera:?}");
        }
        assert_eq!(save.shown(Modifier::Ctrl), "Ctrl + Shift + 1");
        assert_eq!(go.shown(Modifier::Super), "Super + 1");
    }

    #[test]
    fn the_modifier_keys_are_known_by_their_xkb_code() {
        for code in [37, 105, 50, 62, 64, 108, 133, 134] {
            assert!(super::is_modifier_code(code), "{code} is a modifier");
        }
        for code in [38, 54, 65, 9, 66] {
            assert!(!super::is_modifier_code(code), "{code} is not");
        }
    }

    #[test]
    fn ctrl_alt_f_keys_name_their_terminal() {
        assert_eq!(vt_number(0x1008_fe01), Some(1));
        assert_eq!(vt_number(0x1008_fe0c), Some(12));
        assert_eq!(vt_number(0x1008_fe0d), None);
        assert_eq!(vt_number(0x1008_fe00), None);
        assert_eq!(vt_number(0xffbe), None);
    }

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
        assert_eq!(lock.shown(crate::config::Modifier::Super), "Super + L");
        assert_eq!(lock.shown(crate::config::Modifier::Ctrl), "Ctrl + L");
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
    fn the_search_key_opens_search() {
        let names = ["XF86Search".to_string()];
        assert_eq!(built_in(Mods::default(), &names), Some(Action::Search));
        let shifted = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(built_in(shifted, &names), None);
        assert_eq!(built_in(Mods::default(), &["a".to_string()]), None);
    }

    #[test]
    fn keys_read_well() {
        let combo = Combo::parse("ctrl+shift+Return").unwrap().unwrap();
        assert_eq!(combo.shown(crate::config::Modifier::Super), "Ctrl + Shift + Enter");
    }
}
