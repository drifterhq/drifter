use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use drifter_core::users::{self, Users};

const SERVICE: &str = "org.freedesktop.Accounts";
const ROOT: &str = "/org/freedesktop/Accounts";
const USER: &str = "org.freedesktop.Accounts.User";
const WAIT: &str = "--timeout=300";

pub enum Job {
    Rename(String),
    Face(PathBuf),
    Password { old: String, new: String },
    Add {
        full: String,
        login: String,
        password: String,
        admin: bool,
        picture: Option<PathBuf>,
    },
    Admin { login: String, admin: bool },
    Remove { uid: u32, files: bool },
}

fn busctl(args: &[&str]) -> Result<Vec<Vec<String>>, String> {
    let out = Command::new("busctl")
        .args(["--system", WAIT])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("busctl did not run: {error}"))?;
    match out.status.success() {
        true => Ok(users::busctl_lines(&String::from_utf8_lossy(&out.stdout))),
        false => Err(refused(&String::from_utf8_lossy(&out.stderr))),
    }
}

fn refused(said: &str) -> String {
    let line = said.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("");
    if line.to_lowercase().contains("not authorized") || line.contains("Authentication") {
        return drifter_core::lang::t("Not done. The admin password was not given.").to_string();
    }
    match line.trim_start_matches("Call failed: ") {
        "" => drifter_core::lang::t("It did not work.").to_string(),
        rest => rest.to_string(),
    }
}

fn find(login: &str) -> Result<String, String> {
    let lines = busctl(&["call", SERVICE, ROOT, SERVICE, "FindUserByName", "s", login])?;
    lines
        .first()
        .and_then(|words| users::value(words))
        .map(str::to_string)
        .ok_or_else(|| drifter_core::lang::tf("AccountsService does not know {}.", &[&(login)]))
}

fn call(path: &str, method: &str, args: &[&str]) -> Result<(), String> {
    let mut all = vec!["call", SERVICE, path, USER, method];
    all.extend_from_slice(args);
    busctl(&all).map(drop)
}

fn service() -> bool {
    busctl(&["call", SERVICE, ROOT, SERVICE, "ListCachedUsers"]).is_ok()
}

fn my_uid() -> Option<u32> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn my_login() -> Result<String, String> {
    let uid = my_uid().ok_or(drifter_core::lang::t("Cannot tell who you are."))?;
    let passwd = std::fs::read_to_string("/etc/passwd").map_err(|error| error.to_string())?;
    passwd
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|fields| fields.get(2).and_then(|id| id.parse::<u32>().ok()) == Some(uid))
        .and_then(|fields| fields.first().map(|login| login.to_string()))
        .ok_or_else(|| drifter_core::lang::t("Cannot find your account.").to_string())
}

pub fn read() -> Users {
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let group = std::fs::read_to_string("/etc/group").unwrap_or_default();
    let service = service();
    let mut everyone = users::people(&passwd, &group);
    for user in &mut everyone {
        let known = match service {
            true => find(&user.login).and_then(|path| {
                busctl(&["get-property", SERVICE, &path, USER, "RealName", "AccountType", "IconFile"])
            }),
            false => Err(String::new()),
        };
        if let Ok(lines) = known
            && let [name, kind, icon] = lines.as_slice()
        {
            if let Some(name) = users::value(name).filter(|name| !name.trim().is_empty()) {
                user.full = name.trim().to_string();
            }
            user.admin = users::value(kind) == Some("1");
            user.face = users::value(icon)
                .map(PathBuf::from)
                .filter(|path| path.is_file());
        }
        if user.face.is_none() {
            user.face = Some(user.home.join(".face")).filter(|path| path.is_file());
        }
    }
    let (me, others) = users::split(everyone, my_uid().unwrap_or(u32::MAX));
    Users {
        me,
        others,
        taken: users::logins(&passwd),
        service,
    }
}

fn cache() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| crate::window::home().join(".cache"));
    base.join("drifter")
}

fn prepared(picture: &Path, name: &str) -> Result<PathBuf, String> {
    let image = drifter_core::image::load(picture)?;
    let square = users::square(&image, users::FACE);
    let folder = cache();
    std::fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let made = folder.join(name);
    drifter_core::image::save_png(&square, &made)?;
    Ok(made)
}

fn passwd(old: &str, new: &str) -> Result<(), String> {
    let mut child = Command::new("passwd")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("passwd did not run: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = write!(stdin, "{old}\n{new}\n{new}\n");
    }
    let out = child
        .wait_with_output()
        .map_err(|error| format!("passwd did not finish: {error}"))?;
    let said = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    users::passwd_said(out.status.success(), &said)
}

fn hashed(password: &str) -> Result<String, String> {
    let mut child = Command::new("openssl")
        .args(["passwd", "-6", "-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| drifter_core::lang::t("Adding a user needs openssl. Install it, then try again.").to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{password}");
    }
    let out = child
        .wait_with_output()
        .map_err(|error| format!("openssl did not finish: {error}"))?;
    let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match out.status.success() && hash.starts_with("$6$") {
        true => Ok(hash),
        false => Err("openssl could not make the password.".to_string()),
    }
}

fn need_service() -> Result<(), String> {
    match service() {
        true => Ok(()),
        false => Err(drifter_core::lang::t("Install accountsservice to change this.").to_string()),
    }
}

pub fn run(job: Job) -> Result<String, String> {
    match job {
        Job::Rename(full) => {
            need_service()?;
            call(&find(&my_login()?)?, "SetRealName", &["s", &full])?;
            Ok(drifter_core::lang::tf("Your full name is now {}.", &[&(full)]))
        }
        Job::Face(picture) => {
            let made = prepared(&picture, "face.png")?;
            let face = crate::window::home().join(".face");
            if let Err(error) = std::fs::copy(&made, &face) {
                log!("drifter-settings: cannot write {}: {error}", face.display());
            }
            if service() {
                call(&find(&my_login()?)?, "SetIconFile", &["s", &made.to_string_lossy()])?;
            }
            Ok(drifter_core::lang::t("Your picture is changed.").to_string())
        }
        Job::Password { old, new } => {
            passwd(&old, &new)?;
            Ok(drifter_core::lang::t("Your password is changed.").to_string())
        }
        Job::Add {
            full,
            login,
            password,
            admin,
            picture,
        } => {
            need_service()?;
            let hash = hashed(&password)?;
            let kind = if admin { "1" } else { "0" };
            let made = busctl(&["call", SERVICE, ROOT, SERVICE, "CreateUser", "ssi", &login, &full, kind])?;
            let path = made
                .first()
                .and_then(|words| users::value(words))
                .map(str::to_string)
                .map_or_else(|| find(&login), Ok)?;
            call(&path, "SetPassword", &["ss", &hash, ""])
                .map_err(|error| drifter_core::lang::tf("{} was added, but the password was not set: {}", &[&(full), &(error)]))?;
            if let Some(picture) = picture {
                let set = prepared(&picture, &format!("face-{login}.png"))
                    .and_then(|made| call(&path, "SetIconFile", &["s", &made.to_string_lossy()]));
                if let Err(error) = set {
                    log!("drifter-settings: {login} has no picture: {error}");
                }
            }
            Ok(drifter_core::lang::tf("{} can now log in as {}.", &[&(full), &(login)]))
        }
        Job::Admin { login, admin } => {
            need_service()?;
            let kind = if admin { "1" } else { "0" };
            call(&find(&login)?, "SetAccountType", &["i", kind])?;
            Ok(match admin {
                true => drifter_core::lang::tf("{} is now an admin.", &[&(login)]),
                false => drifter_core::lang::tf("{} is now a standard user.", &[&(login)]),
            })
        }
        Job::Remove { uid, files } => {
            need_service()?;
            let wipe = if files { "true" } else { "false" };
            busctl(&["call", SERVICE, ROOT, SERVICE, "DeleteUser", "xb", &uid.to_string(), wipe])?;
            Ok(match files {
                true => drifter_core::lang::t("The user and their home folder are removed.").to_string(),
                false => drifter_core::lang::t("The user is removed. Their home folder is kept.").to_string(),
            })
        }
    }
}
