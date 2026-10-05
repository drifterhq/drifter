#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Once,
    Locked,
}

#[derive(Debug, Default, Clone)]
pub struct Sticky {
    down: Vec<(u32, bool)>,
    held: Vec<(u32, Hold)>,
    sent: Vec<u32>,
}

impl Sticky {
    pub fn held(&self) -> Vec<(u32, Hold)> {
        self.held.clone()
    }

    pub fn clear(&mut self) -> Vec<(u32, bool)> {
        self.held.clear();
        self.down.clear();
        self.sent.drain(..).map(|code| (code, false)).collect()
    }

    pub fn key(&mut self, code: u32, pressed: bool, modifier: bool) -> Vec<(u32, bool)> {
        if modifier {
            return self.modifier(code, pressed);
        }
        if pressed {
            for (_, clean) in &mut self.down {
                *clean = false;
            }
            let mut out: Vec<(u32, bool)> = Vec::new();
            for (held, _) in &self.held {
                let physical = self.down.iter().any(|(down, _)| down == held);
                if !physical && !self.sent.contains(held) {
                    self.sent.push(*held);
                    out.push((*held, true));
                }
            }
            out.push((code, true));
            return out;
        }
        let mut out = vec![(code, false)];
        if !self.sent.is_empty() {
            for sent in self.sent.drain(..) {
                out.push((sent, false));
            }
            self.held.retain(|(_, hold)| *hold == Hold::Locked);
        }
        out
    }

    fn modifier(&mut self, code: u32, pressed: bool) -> Vec<(u32, bool)> {
        if pressed {
            if !self.down.iter().any(|(down, _)| *down == code) {
                self.down.push((code, true));
            }
            if self.sent.contains(&code) {
                return Vec::new();
            }
            return vec![(code, true)];
        }
        let clean = self
            .down
            .iter()
            .find(|(down, _)| *down == code)
            .is_some_and(|(_, clean)| *clean);
        self.down.retain(|(down, _)| *down != code);
        if clean {
            let at = self.held.iter().position(|(held, _)| *held == code);
            match at.map(|at| self.held[at].1) {
                None => self.held.push((code, Hold::Once)),
                Some(Hold::Once) => {
                    let at = at.unwrap_or_default();
                    self.held[at].1 = Hold::Locked;
                }
                Some(Hold::Locked) => self.held.retain(|(held, _)| *held != code),
            }
        }
        match self.sent.contains(&code) {
            true => Vec::new(),
            false => vec![(code, false)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = 29;
    const SHIFT: u32 = 42;
    const C: u32 = 46;
    const V: u32 = 47;

    fn tap(sticky: &mut Sticky, code: u32, modifier: bool) -> Vec<(u32, bool)> {
        let mut out = sticky.key(code, true, modifier);
        out.extend(sticky.key(code, false, modifier));
        out
    }

    #[test]
    fn a_tapped_modifier_holds_for_the_next_key_only() {
        let mut sticky = Sticky::default();
        assert_eq!(tap(&mut sticky, CTRL, true), [(CTRL, true), (CTRL, false)]);
        assert_eq!(sticky.held(), [(CTRL, Hold::Once)]);
        assert_eq!(
            tap(&mut sticky, C, false),
            [(CTRL, true), (C, true), (C, false), (CTRL, false)]
        );
        assert!(sticky.held().is_empty());
        assert_eq!(tap(&mut sticky, V, false), [(V, true), (V, false)]);
    }

    #[test]
    fn two_taps_lock_and_a_third_lets_go() {
        let mut sticky = Sticky::default();
        tap(&mut sticky, SHIFT, true);
        tap(&mut sticky, SHIFT, true);
        assert_eq!(sticky.held(), [(SHIFT, Hold::Locked)]);
        for key in [C, V] {
            assert_eq!(
                tap(&mut sticky, key, false),
                [(SHIFT, true), (key, true), (key, false), (SHIFT, false)]
            );
        }
        tap(&mut sticky, SHIFT, true);
        assert!(sticky.held().is_empty());
        assert_eq!(tap(&mut sticky, C, false), [(C, true), (C, false)]);
    }

    #[test]
    fn a_modifier_held_through_a_key_is_an_ordinary_chord() {
        let mut sticky = Sticky::default();
        let mut out = sticky.key(CTRL, true, true);
        out.extend(tap(&mut sticky, C, false));
        out.extend(sticky.key(CTRL, false, true));
        assert_eq!(out, [(CTRL, true), (C, true), (C, false), (CTRL, false)]);
        assert!(sticky.held().is_empty(), "a chord does not latch");
    }

    #[test]
    fn two_tapped_modifiers_both_wait_for_the_key() {
        let mut sticky = Sticky::default();
        tap(&mut sticky, CTRL, true);
        tap(&mut sticky, SHIFT, true);
        assert_eq!(
            tap(&mut sticky, C, false),
            [(CTRL, true), (SHIFT, true), (C, true), (C, false), (CTRL, false), (SHIFT, false)]
        );
    }

    #[test]
    fn clearing_lets_go_of_anything_sent() {
        let mut sticky = Sticky::default();
        tap(&mut sticky, CTRL, true);
        sticky.key(C, true, false);
        assert_eq!(sticky.clear(), [(CTRL, false)]);
        assert!(sticky.held().is_empty());
    }
}
