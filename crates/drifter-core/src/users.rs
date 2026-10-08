use std::path::{Path, PathBuf};

use crate::image::Image;

pub const FIRST_UID: u32 = 1000;
pub const LAST_UID: u32 = 59999;
pub const ADMIN_GROUPS: [&str; 3] = ["sudo", "wheel", "admin"];
pub const LONGEST_LOGIN: usize = 32;
pub const FACE: u32 = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub login: String,
    pub full: String,
    pub uid: u32,
    pub home: PathBuf,
    pub admin: bool,
    pub face: Option<PathBuf>,
}

impl User {
    pub fn shown(&self) -> &str {
        match self.full.trim().is_empty() {
            true => &self.login,
            false => self.full.trim(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self.admin {
            true => crate::lang::t("Admin"),
            false => crate::lang::t("Standard"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Users {
    pub me: Option<User>,
    pub others: Vec<User>,
    pub taken: Vec<String>,
    pub service: bool,
}

pub fn logins(passwd: &str) -> Vec<String> {
    passwd
        .lines()
        .filter_map(|line| line.split(':').next())
        .filter(|login| !login.is_empty() && !login.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub fn admins(group: &str) -> Vec<String> {
    group
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .filter(|fields| fields.len() >= 4 && ADMIN_GROUPS.contains(&fields[0]))
        .flat_map(|fields| {
            fields[3]
                .split(',')
                .map(str::trim)
                .filter(|member| !member.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn can_log_in(shell: &str) -> bool {
    let shell = shell.trim();
    !shell.is_empty() && !shell.ends_with("/nologin") && !shell.ends_with("/false")
}

pub fn people(passwd: &str, group: &str) -> Vec<User> {
    let admins = admins(group);
    passwd
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            let [login, _, uid, _, gecos, home, shell] = fields.as_slice() else {
                return None;
            };
            let uid: u32 = uid.parse().ok()?;
            let real = (FIRST_UID..=LAST_UID).contains(&uid) && can_log_in(shell);
            real.then(|| User {
                login: login.to_string(),
                full: gecos.split(',').next().unwrap_or("").trim().to_string(),
                uid,
                home: PathBuf::from(home),
                admin: admins.iter().any(|admin| admin == login),
                face: None,
            })
        })
        .collect()
}

pub fn split(mut everyone: Vec<User>, me: u32) -> (Option<User>, Vec<User>) {
    everyone.sort_by_key(|user| user.shown().to_lowercase());
    let at = everyone.iter().position(|user| user.uid == me);
    let mine = at.map(|at| everyone.remove(at));
    (mine, everyone)
}

pub fn busctl_lines(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(busctl_words)
        .filter(|words| !words.is_empty())
        .collect()
}

pub fn busctl_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if c == '"' {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => match chars.next() {
                        Some('n') => word.push('\n'),
                        Some('t') => word.push('\t'),
                        Some(other) => word.push(other),
                        None => {}
                    },
                    c => word.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        words.push(word);
    }
    words
}

pub fn value(words: &[String]) -> Option<&str> {
    words.get(1).map(String::as_str)
}

pub fn check_full(full: &str) -> Result<String, String> {
    let full = full.trim();
    if full.is_empty() {
        return Err(crate::lang::t("Type a full name.").to_string());
    }
    if full.contains([':', ',', '\n', '\r', '=']) {
        return Err(crate::lang::t("A full name cannot have a colon, a comma or an equals sign.").to_string());
    }
    if full.chars().count() > 64 {
        return Err(crate::lang::t("A full name can be 64 letters at most.").to_string());
    }
    Ok(full.to_string())
}

pub fn check_login(login: &str, taken: &[String]) -> Result<(), String> {
    if login.is_empty() {
        return Err(crate::lang::t("Type an account name.").to_string());
    }
    if login.len() > LONGEST_LOGIN {
        return Err(crate::lang::tf("An account name can be {} letters at most.", &[&(LONGEST_LOGIN)]));
    }
    let mut chars = login.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if !first_ok || !rest_ok {
        return Err(
            "An account name starts with a small letter and has only small letters, digits, - and _."
                .to_string(),
        );
    }
    if taken.iter().any(|name| name == login) {
        return Err(crate::lang::tf("The account name {} is already taken.", &[&(login)]));
    }
    Ok(())
}

pub fn login_from(full: &str) -> String {
    let kept: String = full
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .skip_while(|c| !c.is_ascii_lowercase())
        .take(LONGEST_LOGIN)
        .collect();
    kept
}

pub fn check_new_password(new: &str, repeat: &str) -> Result<(), String> {
    if new.is_empty() {
        return Err(crate::lang::t("Type a new password.").to_string());
    }
    if new != repeat {
        return Err(crate::lang::t("The two new passwords are not the same.").to_string());
    }
    Ok(())
}

const PASSWD_NOISE: [&str; 6] = [
    "Changing password for",
    "passwd: Authentication token manipulation error",
    "passwd: password unchanged",
    "Password change has been aborted",
    "passwd: Have exhausted maximum number of retries",
    "passwd: password updated successfully",
];

const PROMPTS: [&str; 3] = ["Current password:", "Retype new password:", "New password:"];

pub fn passwd_said(worked: bool, said: &str) -> Result<(), String> {
    if worked {
        return Ok(());
    }
    if !said.contains("New password:") {
        return Err(crate::lang::t("The current password is wrong.").to_string());
    }
    let mut text = said.to_string();
    for prompt in PROMPTS {
        text = text.replace(prompt, "\n");
    }
    let reason = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| !PASSWD_NOISE.iter().any(|noise| line.starts_with(noise)));
    Err(match reason {
        Some(line) => match line.strip_prefix("BAD PASSWORD:") {
            Some(why) => crate::lang::tf("The new password is not allowed: {}.", &[&(why.trim().trim_end_matches('.'))]),
            None if line.contains("longer password") => {
                crate::lang::t("The new password is too short. Choose a longer one.").to_string()
            }
            None if line.contains("do not match") => {
                crate::lang::t("The two new passwords are not the same.").to_string()
            }
            None => line.to_string(),
        },
        None => crate::lang::t("The password was not changed.").to_string(),
    })
}

pub fn stock(folder: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg"))
        })
        .collect();
    found.sort();
    found
}

pub fn pick_stock(stock: &[PathBuf], seed: u64) -> Option<PathBuf> {
    match stock.len() {
        0 => None,
        count => stock.get((seed % count as u64) as usize).cloned(),
    }
}

pub fn square(image: &Image, side: u32) -> Image {
    crate::settings::cover(image, side, side)
}

pub fn round(image: &Image) -> Image {
    let mut out = image.clone();
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    let center = (w / 2.0, h / 2.0);
    let radius = w.min(h) / 2.0;
    for y in 0..image.height {
        for x in 0..image.width {
            let dx = f64::from(x) + 0.5 - center.0;
            let dy = f64::from(y) + 0.5 - center.1;
            let keep = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            if keep >= 1.0 {
                continue;
            }
            let at = ((y * image.width + x) * 4) as usize;
            for channel in &mut out.rgba[at..at + 4] {
                *channel = (f64::from(*channel) * keep).round() as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
        daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
        dh:x:1000:1000:Dee H,,,:/home/dh:/bin/bash\n\
        sam:x:1001:1001::/home/sam:/bin/zsh\n\
        ghost:x:1002:1002:Ghost:/home/ghost:/usr/sbin/nologin\n\
        nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n\
        builder:x:1003:1003:Build Bot:/srv/b:/bin/false\n";

    const GROUP: &str = "root:x:0:\nsudo:x:27:dh\nwheel:x:10:\nusers:x:100:sam,dh\n";

    #[test]
    fn only_real_people_are_listed_with_their_admin_rights() {
        let people = people(PASSWD, GROUP);
        let found: Vec<(&str, &str, bool)> = people
            .iter()
            .map(|user| (user.login.as_str(), user.full.as_str(), user.admin))
            .collect();
        assert_eq!(found, vec![("dh", "Dee H", true), ("sam", "", false)]);
        assert_eq!(people[1].shown(), "sam");
        assert_eq!(people[0].kind(), "Admin");
        assert_eq!(people[1].kind(), "Standard");
        assert_eq!(people[0].home, PathBuf::from("/home/dh"));
    }

    #[test]
    fn admins_come_from_sudo_wheel_and_admin() {
        let group = "sudo:x:27:dh, ann\nwheel:x:10:bo\nadmin:x:11:\nstaff:x:50:zed\n";
        assert_eq!(admins(group), vec!["dh", "ann", "bo"]);
    }

    #[test]
    fn every_login_counts_as_taken() {
        let taken = logins(PASSWD);
        assert!(taken.contains(&"root".to_string()));
        assert!(taken.contains(&"nobody".to_string()));
        assert_eq!(taken.len(), 7);
    }

    #[test]
    fn i_am_split_from_the_others_and_they_sort_by_name() {
        let mut everyone = people(PASSWD, GROUP);
        everyone.push(User {
            login: "ann".into(),
            full: "Ann".into(),
            uid: 1009,
            home: PathBuf::from("/home/ann"),
            admin: false,
            face: None,
        });
        let (me, others) = split(everyone, 1001);
        assert_eq!(me.map(|user| user.login), Some("sam".to_string()));
        let names: Vec<&str> = others.iter().map(User::shown).collect();
        assert_eq!(names, vec!["Ann", "Dee H"]);
    }

    #[test]
    fn busctl_output_splits_into_words_with_quotes_kept_whole() {
        let text = "s \"Dee \\\"D\\\" H\"\ni 1\nao 2 \"/a/User1000\" \"/a/User1001\"\nb true\ns \"\"\n";
        let lines = busctl_lines(text);
        assert_eq!(lines[0], vec!["s", "Dee \"D\" H"]);
        assert_eq!(value(&lines[1]), Some("1"));
        assert_eq!(lines[2], vec!["ao", "2", "/a/User1000", "/a/User1001"]);
        assert_eq!(value(&lines[3]), Some("true"));
        assert_eq!(lines[4], vec!["s", ""]);
    }

    #[test]
    fn a_full_name_is_trimmed_and_cannot_break_the_passwd_file() {
        assert_eq!(check_full("  Ann Lee "), Ok("Ann Lee".to_string()));
        assert!(check_full("   ").is_err());
        assert!(check_full("Ann, Lee").is_err());
        assert!(check_full("Ann:Lee").is_err());
        assert!(check_full(&"a".repeat(65)).is_err());
    }

    #[test]
    fn account_names_follow_the_debian_rules() {
        let taken = vec!["dh".to_string(), "root".to_string()];
        assert_eq!(check_login("ann", &taken), Ok(()));
        assert_eq!(check_login("ann-lee_2", &taken), Ok(()));
        assert!(check_login("", &taken).is_err());
        assert!(check_login("Ann", &taken).is_err());
        assert!(check_login("2ann", &taken).is_err());
        assert!(check_login("ann lee", &taken).is_err());
        assert!(check_login(&"a".repeat(33), &taken).is_err());
        let said = check_login("dh", &taken).unwrap_err();
        assert!(said.contains("taken"), "{said}");
    }

    #[test]
    fn the_account_name_is_made_from_the_full_name() {
        assert_eq!(login_from("Ann Lee"), "annlee");
        assert_eq!(login_from("  José  Núñez 2 "), "josnez2");
        assert_eq!(login_from("42 Bob"), "bob");
        assert_eq!(login_from("!!!"), "");
        assert_eq!(login_from(&"x".repeat(40)).len(), LONGEST_LOGIN);
        assert_eq!(check_login(&login_from("Ann Lee"), &[]), Ok(()));
    }

    #[test]
    fn new_passwords_must_match_and_not_be_empty() {
        assert_eq!(check_new_password("a-b-c-d", "a-b-c-d"), Ok(()));
        assert!(check_new_password("", "").is_err());
        assert!(check_new_password("one", "two").is_err());
    }

    #[test]
    fn what_passwd_said_becomes_a_clear_reason() {
        let wrong = "Current password: Changing password for drtest.\n\
            passwd: Authentication token manipulation error\npasswd: password unchanged\n";
        assert_eq!(
            passwd_said(false, wrong),
            Err("The current password is wrong.".to_string())
        );
        let short = "passwd: Authentication token manipulation error\npasswd: password unchanged\n\
            Current password: New password: Retype new password: You must choose a longer password.\n\
            New password: Password change has been aborted.\nChanging password for drtest.\n";
        assert_eq!(
            passwd_said(false, short),
            Err("The new password is too short. Choose a longer one.".to_string())
        );
        let simple = "Current password: New password: Retype new password: \
            BAD PASSWORD: it is too simplistic/systematic\nNew password: Password change has been aborted.\n";
        assert_eq!(
            passwd_said(false, simple),
            Err("The new password is not allowed: it is too simplistic/systematic.".to_string())
        );
        let good = "Current password: New password: Retype new password: Changing password for drtest.\n\
            passwd: password updated successfully\n";
        assert_eq!(passwd_said(true, good), Ok(()));
        let odd = "Current password: New password: Retype new password: \n";
        assert_eq!(
            passwd_said(false, odd),
            Err("The password was not changed.".to_string())
        );
    }

    #[test]
    fn stock_pictures_are_the_png_and_jpeg_files_in_order() {
        let folder = std::env::temp_dir().join(format!("drifter-faces-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["b-cat.png", "a-ball.jpg", "notes.txt", "c-tree.PNG"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let found: Vec<String> = stock(&folder)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(found, vec!["a-ball.jpg", "b-cat.png", "c-tree.PNG"]);
        assert!(stock(Path::new("/no/such/folder")).is_empty());
        let paths: Vec<PathBuf> = ["a", "b", "c"].iter().map(PathBuf::from).collect();
        assert_eq!(pick_stock(&paths, 7), Some(PathBuf::from("b")));
        assert_eq!(pick_stock(&[], 7), None);
    }

    #[test]
    fn a_round_picture_is_clear_in_the_corners_and_full_in_the_middle() {
        let image = Image {
            width: 20,
            height: 20,
            rgba: vec![200; 20 * 20 * 4],
        };
        let round = round(&image);
        assert_eq!(&round.rgba[0..4], &[0, 0, 0, 0]);
        let middle = ((10 * 20 + 10) * 4) as usize;
        assert_eq!(&round.rgba[middle..middle + 4], &[200, 200, 200, 200]);
    }
}
