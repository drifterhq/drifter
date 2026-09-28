use crate::menu::Item;

const FIRST_ID: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Logout,
    Shutdown,
    Suspend,
    Reboot,
}

impl Action {
    pub const ALL: [Action; 4] = [
        Action::Logout,
        Action::Shutdown,
        Action::Suspend,
        Action::Reboot,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Logout => "Log out",
            Action::Shutdown => "Shut down",
            Action::Suspend => "Suspend",
            Action::Reboot => "Restart",
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
            Action::Logout => None,
            Action::Shutdown => Some("PowerOff"),
            Action::Suspend => Some("Suspend"),
            Action::Reboot => Some("Reboot"),
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
    fn the_menu_lists_logout_first_and_restart_last() {
        let labels: Vec<String> = items().into_iter().map(|item| item.label).collect();
        assert_eq!(labels, ["Log out", "Shut down", "Suspend", "Restart"]);
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
        assert_eq!(Action::from_id(FIRST_ID + 4), None);
    }

    #[test]
    fn only_logout_stays_inside_drifter() {
        assert_eq!(Action::Logout.method(), None);
        assert_eq!(Action::Shutdown.method(), Some("PowerOff"));
        assert_eq!(Action::Suspend.method(), Some("Suspend"));
        assert_eq!(Action::Reboot.method(), Some("Reboot"));
    }
}
