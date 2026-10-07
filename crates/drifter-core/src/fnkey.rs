pub const SEARCH: u32 = 225;

pub fn top_row(code: u32) -> Option<u32> {
    Some(match code {
        67 => 166,
        68 => 181,
        69 => 95,
        70 => 128,
        71 => 232,
        72 => 233,
        73 => 172,
        74 => 121,
        75 => 122,
        76 => 123,
        _ => return None,
    })
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FnKey {
    down: bool,
    used: bool,
    passed: bool,
    turned: Vec<(u32, u32)>,
}

impl FnKey {
    pub fn key(&mut self, code: u32, pressed: bool) -> Vec<(u32, bool)> {
        if code == SEARCH {
            return match pressed {
                true => {
                    if self.down {
                        return Vec::new();
                    }
                    *self = FnKey { down: true, turned: std::mem::take(&mut self.turned), ..FnKey::default() };
                    Vec::new()
                }
                false => {
                    let out = match (self.used, self.passed) {
                        (_, true) => vec![(SEARCH, false)],
                        (false, false) => vec![(SEARCH, true), (SEARCH, false)],
                        (true, false) => Vec::new(),
                    };
                    self.down = false;
                    out
                }
            };
        }
        if let Some(at) = self.turned.iter().position(|(from, _)| *from == code)
            && !pressed
        {
            let (_, to) = self.turned.swap_remove(at);
            return vec![(to, false)];
        }
        if self.down
            && pressed
            && let Some(to) = top_row(code)
        {
            self.used = true;
            self.turned.push((code, to));
            return vec![(to, true)];
        }
        if self.down && pressed && !self.used && !self.passed {
            self.passed = true;
            return vec![(SEARCH, true), (code, true)];
        }
        vec![(code, pressed)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_row_alone_stays_f_keys() {
        let mut keys = FnKey::default();
        assert_eq!(keys.key(72, true), vec![(72, true)]);
        assert_eq!(keys.key(72, false), vec![(72, false)]);
    }

    #[test]
    fn holding_search_turns_the_top_row_into_media_keys() {
        let mut keys = FnKey::default();
        assert!(keys.key(SEARCH, true).is_empty());
        assert_eq!(keys.key(72, true), vec![(233, true)], "F6 is brightness up");
        assert_eq!(keys.key(72, false), vec![(233, false)]);
        assert_eq!(keys.key(76, true), vec![(123, true)], "F10 is volume up");
        assert_eq!(keys.key(76, false), vec![(123, false)]);
        assert!(keys.key(SEARCH, false).is_empty(), "no Spotlight after using it as Fn");
    }

    #[test]
    fn a_tap_of_search_still_opens_spotlight() {
        let mut keys = FnKey::default();
        assert!(keys.key(SEARCH, true).is_empty());
        assert_eq!(keys.key(SEARCH, false), vec![(SEARCH, true), (SEARCH, false)]);
    }

    #[test]
    fn a_media_key_released_after_search_still_lets_go() {
        let mut keys = FnKey::default();
        keys.key(SEARCH, true);
        keys.key(71, true);
        keys.key(SEARCH, false);
        assert_eq!(keys.key(71, false), vec![(232, false)]);
        assert_eq!(keys.key(71, true), vec![(71, true)], "back to F5 once search is up");
    }

    #[test]
    fn search_with_another_key_still_reaches_apps_as_search() {
        let mut keys = FnKey::default();
        keys.key(SEARCH, true);
        assert_eq!(keys.key(46, true), vec![(SEARCH, true), (46, true)]);
        assert_eq!(keys.key(46, false), vec![(46, false)]);
        assert_eq!(keys.key(SEARCH, false), vec![(SEARCH, false)]);
    }

    #[test]
    fn a_held_search_repeating_does_not_reset() {
        let mut keys = FnKey::default();
        keys.key(SEARCH, true);
        keys.key(72, true);
        assert!(keys.key(SEARCH, true).is_empty());
        assert!(keys.key(SEARCH, false).is_empty());
    }
}
