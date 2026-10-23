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
            Action::Suspend => Some("Suspend"),
            Action::Reboot => Some("Reboot"),
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

impl Action {
    pub fn glyph(self) -> crate::rows::Glyph {
        match self {
            Action::Lock => crate::rows::Glyph::Lock,
            Action::Logout => crate::rows::Glyph::LogOut,
            Action::Shutdown => crate::rows::Glyph::Power,
            Action::Suspend => crate::rows::Glyph::Sleep,
            Action::Reboot => crate::rows::Glyph::Restart,
        }
    }
}

pub fn items(lock_keys: Option<String>) -> Vec<Item> {
    let row = |action: Action| Item::line(action.id(), action.label()).glyph(action.glyph());
    let lock = match lock_keys {
        Some(keys) => row(Action::Lock).trail(crate::rows::Trail::Shortcut(keys)),
        None => row(Action::Lock),
    };
    vec![
        lock,
        row(Action::Suspend),
        Item::rule(90),
        row(Action::Reboot),
        row(Action::Shutdown),
        Item::rule(91),
        row(Action::Logout),
    ]
}
