use std::collections::HashMap;

use super::{Ask, Control, Fonts, Group, Id, Look, Mark, Model, Row};
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::sheet::face_key;

pub const ROW_FACE: u32 = 26;
pub const BIG_FACE: u32 = 112;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAsk {
    Rename,
    Picture,
    Password,
    Add,
    Admin(usize, bool),
    Remove(usize),
}

impl Model {
    pub(super) fn user_groups(&self) -> Vec<Group> {
        let Some(users) = &self.users else {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some(crate::lang::t("Reading the accounts…").into()),
                thumbs: None,
            }];
        };
        let change = || Control::Buttons(vec![Mark::Text(crate::lang::t("Change"))]);
        let mut groups = Vec::new();
        if let Some(me) = &users.me {
            let full = match me.full.trim().is_empty() {
                true => crate::lang::t("Not set"),
                false => me.full.trim(),
            };
            groups.push(Group {
                header: Some(crate::lang::t("Your account")),
                rows: vec![
                    Row::new(Id::MyName, crate::lang::t("Full name"), change()).detail(full),
                    Row::new(Id::MyPicture, crate::lang::t("Picture"), change())
                        .detail(crate::lang::t("Shown when you log in")),
                    Row::new(Id::MyPassword, crate::lang::t("Password"), change())
                        .detail(crate::lang::t("Asks for your current password first")),
                ],
                footer: self.user_note.clone(),
                thumbs: None,
            });
        }
        let mut rows: Vec<Row> = users
            .others
            .iter()
            .enumerate()
            .map(|(at, user)| {
                let flip = match user.admin {
                    true => crate::lang::t("Make Standard"),
                    false => crate::lang::t("Make Admin"),
                };
                let mut row = Row::new(
                    Id::User(at),
                    user.shown(),
                    Control::Buttons(vec![Mark::Text(flip), Mark::Text(crate::lang::t("Remove"))]),
                )
                .detail(&format!("{} · {}", user.login, user.kind()))
                .enabled(users.service);
                row.icon = Some(match &user.face {
                    Some(face) => face_key(face, ROW_FACE),
                    None => format!("letter:{}", user.login),
                });
                row
            })
            .collect();
        rows.push(
            Row::new(Id::AddUser, crate::lang::t("Add a user…"), Control::Buttons(vec![Mark::Plus]))
                .enabled(users.service),
        );
        let footer = match (users.service, users.me.is_some()) {
            (false, _) => crate::lang::t("Install accountsservice to add users or change them."),
            (true, _) if users.others.is_empty() => {
                crate::lang::t("No other users yet. Adding a user asks for an admin password.")
            }
            (true, _) => crate::lang::t("Adding, removing or changing another user asks for an admin password."),
        };
        groups.push(Group {
            header: Some(crate::lang::t("Other users")),
            rows,
            footer: Some(match users.me {
                Some(_) => footer.to_string(),
                None => self.user_note.clone().unwrap_or_else(|| footer.to_string()),
            }),
            thumbs: None,
        });
        groups
    }

    pub(super) fn press_user(&mut self, id: Id, index: usize) -> Option<Ask> {
        let users = self.users.as_ref()?;
        let others = users.others.len();
        let ask = match (id, index) {
            (Id::MyName, _) if users.me.is_some() => UserAsk::Rename,
            (Id::MyPicture, _) if users.me.is_some() => UserAsk::Picture,
            (Id::MyPassword, _) if users.me.is_some() => UserAsk::Password,
            (Id::AddUser, _) => UserAsk::Add,
            (Id::User(at), 0) if at < others => UserAsk::Admin(at, !users.others[at].admin),
            (Id::User(at), _) if at < others => UserAsk::Remove(at),
            _ => return None,
        };
        Some(Ask::User(ask))
    }

    pub fn me(&self) -> Option<&crate::users::User> {
        self.users.as_ref().and_then(|users| users.me.as_ref())
    }

    pub fn other(&self, at: usize) -> Option<&crate::users::User> {
        self.users.as_ref().and_then(|users| users.others.get(at))
    }

    pub fn face_keys(&self) -> Vec<String> {
        let Some(users) = &self.users else {
            return Vec::new();
        };
        let mine = users
            .me
            .as_ref()
            .and_then(|me| me.face.as_ref())
            .map(|face| face_key(face, BIG_FACE));
        let theirs = users
            .others
            .iter()
            .filter_map(|user| user.face.as_ref())
            .map(|face| face_key(face, ROW_FACE));
        mine.into_iter().chain(theirs).collect()
    }
}

impl Look {
    pub fn face_spot(&self, card: Rect) -> Rect {
        let side = f64::from(BIG_FACE);
        Rect::new(
            Vec2::new(card.origin.x + 30.0, card.center().y - side / 2.0),
            Vec2::splat(side),
        )
    }

    pub(super) fn paint_me(
        &self,
        canvas: &mut Canvas,
        card: Rect,
        model: &Model,
        fonts: &Fonts,
        icons: &HashMap<String, Image>,
    ) {
        let spot = self.face_spot(card);
        let me = model.me();
        crate::sheet::paint_face(canvas, self, icons, spot, me.and_then(|me| me.face.as_deref()));
        let x = spot.max().x + 26.0;
        let room = card.max().x - x - 20.0;
        let Some(me) = me else {
            let text = fonts.body.paint(crate::lang::t("Reading your account…"), self.dim);
            canvas.stamp(&text, Vec2::new(x, card.center().y - crate::paint::logical(&text).y / 2.0), 1.0);
            return;
        };
        let name = fonts.title.paint(&super::fit(fonts.title, me.shown(), room), self.ink);
        let kind = fonts.body.paint(
            &super::fit(fonts.body, &format!("{} · {}", me.login, me.kind()), room),
            self.dim,
        );
        let hint = fonts.small.paint(crate::lang::t("Click the picture to change it."), self.dim);
        let tall = crate::paint::logical(&name).y + crate::paint::logical(&kind).y + crate::paint::logical(&hint).y + 10.0;
        let mut y = card.center().y - tall / 2.0;
        canvas.stamp(&name, Vec2::new(x, y), 1.0);
        y += crate::paint::logical(&name).y + 4.0;
        canvas.stamp(&kind, Vec2::new(x, y), 1.0);
        y += crate::paint::logical(&kind).y + 6.0;
        canvas.stamp(&hint, Vec2::new(x, y), 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Hit, Section};
    use crate::users::{User, Users};
    use std::path::PathBuf;

    fn person(login: &str, full: &str, uid: u32, admin: bool, face: Option<&str>) -> User {
        User {
            login: login.into(),
            full: full.into(),
            uid,
            home: PathBuf::from(format!("/home/{login}")),
            admin,
            face: face.map(PathBuf::from),
        }
    }

    fn model(service: bool) -> Model {
        let mut model = Model::read(&crate::config::Config::default(), &[], |_| None);
        model.users = Some(Users {
            me: Some(person("dh", "Dee H", 1000, true, Some("/icons/dh"))),
            others: vec![
                person("ann", "Ann Lee", 1001, false, None),
                person("bo", "", 1002, true, Some("/icons/bo")),
            ],
            taken: vec!["dh".into(), "ann".into(), "bo".into(), "root".into()],
            service,
        });
        model
    }

    #[test]
    fn the_page_shows_my_account_then_the_others_and_an_add_row() {
        let model = model(true);
        let groups = model.groups(Section::Users);
        assert_eq!(groups[0].header, Some("Your account"));
        let mine: Vec<(&str, Option<&str>)> = groups[0]
            .rows
            .iter()
            .map(|row| (row.label.as_str(), row.detail.as_deref()))
            .collect();
        assert_eq!(mine[0], ("Full name", Some("Dee H")));
        assert_eq!(mine[1].0, "Picture");
        assert_eq!(mine[2].0, "Password");
        let others = &groups[1].rows;
        assert_eq!(others[0].label, "Ann Lee");
        assert_eq!(others[0].detail.as_deref(), Some("ann · Standard"));
        assert_eq!(others[0].control, Control::Buttons(vec![Mark::Text("Make Admin"), Mark::Text("Remove")]));
        assert_eq!(others[1].label, "bo");
        assert_eq!(others[1].detail.as_deref(), Some("bo · Admin"));
        assert_eq!(others[1].icon.as_deref(), Some("face:26:/icons/bo"));
        assert_eq!(others[0].icon.as_deref(), Some("letter:ann"));
        assert_eq!(others[2].id, Id::AddUser);
        assert!(others.iter().all(|row| row.enabled));
        assert_eq!(model.face_keys(), vec!["face:112:/icons/dh", "face:26:/icons/bo"]);
    }

    #[test]
    fn without_accountsservice_the_others_are_shown_but_greyed() {
        let model = model(false);
        let groups = model.groups(Section::Users);
        assert!(groups[1].rows.iter().all(|row| !row.enabled));
        assert!(groups[1].footer.as_deref().unwrap().contains("accountsservice"));
        assert!(groups[0].rows.iter().all(|row| row.enabled), "my own account still works");
    }

    #[test]
    fn each_button_asks_for_the_right_change() {
        let mut model = model(true);
        assert_eq!(model.press(Id::MyName, 0), Some(Ask::User(UserAsk::Rename)));
        assert_eq!(model.press(Id::MyPicture, 0), Some(Ask::User(UserAsk::Picture)));
        assert_eq!(model.press(Id::MyPassword, 0), Some(Ask::User(UserAsk::Password)));
        assert_eq!(model.press(Id::AddUser, 0), Some(Ask::User(UserAsk::Add)));
        assert_eq!(model.press(Id::User(0), 0), Some(Ask::User(UserAsk::Admin(0, true))));
        assert_eq!(model.press(Id::User(1), 0), Some(Ask::User(UserAsk::Admin(1, false))));
        assert_eq!(model.press(Id::User(1), 1), Some(Ask::User(UserAsk::Remove(1))));
        assert_eq!(model.press(Id::User(7), 1), None);
        model.users = None;
        assert_eq!(model.press(Id::MyName, 0), None);
        assert_eq!(model.groups(Section::Users)[0].footer.as_deref(), Some("Reading the accounts…"));
    }

    #[test]
    fn clicking_my_picture_asks_to_change_it() {
        let model = model(true);
        let look = Look::default();
        let size = Vec2::new(900.0, 640.0);
        let spot = look.face_spot(look.picture_rect(size, 0.0));
        assert_eq!(
            look.hit(size, Section::Users, &model, 0.0, spot.center()),
            Some(Hit::Press(Id::MyPicture, 0))
        );
    }

    #[test]
    fn the_users_page_is_found_by_its_words() {
        let mut model = model(true);
        for word in ["password", "admin", "avatar", "account"] {
            model.query = word.to_string();
            assert!(model.shown().contains(&Section::Users), "{word}");
        }
    }
}
