use crate::menu::Item;

const FIRST_ID: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Lock,
    Logout,
    Shutdown,
    Suspend,
    Reboot,
}

impl Action {
    pub const ALL: [Action; 5] = [
        Action::Lock,
        Action::Logout,
        Action::Shutdown,
        Action::Suspend,
        Action::Reboot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Lock => crate::lang::t("Lock"),
            Action::Logout => crate::lang::t("Log out"),
            Action::Shutdown => crate::lang::t("Shut down"),
            Action::Suspend => crate::lang::t("Suspend"),
            Action::Reboot => crate::lang::t("Restart"),
        }
    }

    pub fn id(self) -> i32 {
        let at = Action::ALL.iter().position(|action| *action == self);
        FIRST_ID + at.unwrap_or(0) as i32
    }

    pub fn from_id(id: i32) -> Option<Action> {
        let at = usize::try_from(id - FIRST_ID).ok()?;
        Action::ALL.get(at).copied()
    }

    pub fn method(self) -> Option<&'static str> {
        match self {
            Action::Lock | Action::Logout => None,
            Action::Shutdown => Some("PowerOff"),
            Action::Suspend => Some(crate::lang::t("Suspend")),
            Action::Reboot => Some(crate::lang::t("Reboot")),
        }
    }
}

pub const COUNTDOWN: u64 = 60;
pub const COUNTDOWN_RANGE: (u64, u64) = (10, 60);

impl Action {
    pub fn notice(self) -> &'static str {
        match self {
            Action::Logout => crate::lang::t("You will be logged out automatically."),
            Action::Reboot => crate::lang::t("Your computer will restart automatically."),
            _ => crate::lang::t("Your computer will shut down automatically."),
        }
    }
}

pub fn items() -> Vec<Item> {
    Action::ALL
        .iter()
        .map(|action| Item::line(action.id(), action.label()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_lists_lock_first_and_restart_last() {
        let labels: Vec<String> = items().into_iter().map(|item| item.label).collect();
        assert_eq!(
            labels,
            ["Lock", "Log out", "Shut down", "Suspend", "Restart"]
        );
    }

    #[test]
    fn every_action_is_found_again_by_its_id() {
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
        }
    }

    #[test]
    fn the_ids_never_meet_the_settings_entry() {
        assert!(Action::ALL.iter().all(|action| action.id() != 0));
        assert_eq!(Action::from_id(0), None);
        assert_eq!(Action::from_id(-1), None);
        assert_eq!(Action::from_id(FIRST_ID + 5), None);
    }

    #[test]
    fn the_notice_says_what_happens() {
        assert_eq!(Action::Reboot.notice(), "Your computer will restart automatically.");
        assert_eq!(Action::Shutdown.notice(), "Your computer will shut down automatically.");
        assert_eq!(Action::Logout.notice(), "You will be logged out automatically.");
    }

    #[test]
    fn lock_and_logout_stay_inside_drifter() {
        assert_eq!(Action::Lock.method(), None);
        assert_eq!(Action::Logout.method(), None);
        assert_eq!(Action::Shutdown.method(), Some("PowerOff"));
        assert_eq!(Action::Suspend.method(), Some("Suspend"));
        assert_eq!(Action::Reboot.method(), Some("Reboot"));
    }
}
