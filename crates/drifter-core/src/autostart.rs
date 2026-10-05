use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub on: bool,
}

impl Entry {
    pub fn program(&self) -> Option<String> {
        program_of(&self.exec)
    }

    pub fn argv(&self) -> Option<Vec<String>> {
        let mut argv = crate::exec::command(&self.exec, &[])?;
        let indicator = "--indicator".to_string();
        if self.program().as_deref() == Some("nm-applet") && !argv.contains(&indicator) {
            argv.push(indicator);
        }
        Some(argv)
    }
}

pub fn program_of(line: &str) -> Option<String> {
    let first = crate::exec::words(line)?.into_iter().next()?;
    Path::new(&first)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

const BUILT_IN: [&str; 9] = [
    "lxpolkit",
    "dunst",
    "mako",
    "xfce4-notifyd",
    "lxqt-notificationd",
    "notification-daemon",
    "notify-osd",
    "xscreensaver",
    "light-locker",
];

pub fn built_in(program: &str) -> bool {
    program.contains("polkit") || program.contains("policykit") || BUILT_IN.contains(&program)
}

pub fn user_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("autostart")
}

pub fn system_dirs() -> Vec<PathBuf> {
    let listed = std::env::var("XDG_CONFIG_DIRS").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = listed
        .split(':')
        .filter(|dir| dir.starts_with('/'))
        .map(|dir| PathBuf::from(dir).join("autostart"))
        .collect();
    if dirs.is_empty() {
        dirs.push(PathBuf::from("/etc/xdg/autostart"));
    }
    dirs
}

pub fn desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_else(|_| "drifter".to_string())
        .split(':')
        .map(str::to_string)
        .collect()
}

pub fn read(
    id: &str,
    text: &str,
    desktops: &[String],
    found: impl Fn(&str) -> bool,
) -> Option<Entry> {
    let mut inside = false;
    let (mut name, mut exec, mut icon, mut kind) = (None, None, None, None);
    let (mut try_exec, mut only, mut not) = (None, None, None);
    let mut on = true;
    let mut systemd = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "Name" => name = Some(value),
            "Exec" => exec = Some(value),
            "Icon" => icon = Some(value),
            "Type" => kind = Some(value),
            "TryExec" => try_exec = Some(value),
            "OnlyShowIn" => only = Some(value),
            "NotShowIn" => not = Some(value),
            "Hidden" if value == "true" => on = false,
            "X-GNOME-Autostart-enabled" if value == "false" => on = false,
            "X-GNOME-HiddenUnderSystemd" if value == "true" => systemd = true,
            _ => {}
        }
    }
    let named = |list: &str| {
        list.split(';')
            .filter(|each| !each.is_empty())
            .any(|each| desktops.iter().any(|desk| desk.eq_ignore_ascii_case(each)))
    };
    if only.as_deref().is_some_and(|list| !named(list))
        || not.as_deref().is_some_and(named)
        || systemd
    {
        return None;
    }
    if kind.as_deref().is_some_and(|kind| kind != "Application") {
        return None;
    }
    if try_exec.as_deref().is_some_and(|program| !found(program)) {
        return None;
    }
    let exec = exec.filter(|exec| !exec.trim().is_empty())?;
    if program_of(&exec).is_none_or(|program| built_in(&program)) {
        return None;
    }
    Some(Entry {
        id: id.to_string(),
        name: name.unwrap_or_else(|| id.to_string()),
        exec,
        icon,
        on,
    })
}

pub fn scan(
    user: &Path,
    system: &[PathBuf],
    desktops: &[String],
    found: impl Fn(&str) -> bool,
) -> Vec<Entry> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for dir in std::iter::once(user).chain(system.iter().map(PathBuf::as_path)) {
        let Ok(files) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = files.flatten().map(|file| file.path()).collect();
        files.sort();
        for path in files {
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if seen.iter().any(|each| each == id) {
                continue;
            }
            seen.push(id.to_string());
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(entry) = read(id, &text, desktops, &found) {
                out.push(entry);
            }
        }
    }
    out.sort_by_key(|entry| entry.name.to_lowercase());
    out
}

pub fn installed() -> Vec<Entry> {
    scan(&user_dir(), &system_dirs(), &desktops(), on_path)
}

pub fn on_path(program: &str) -> bool {
    if program.starts_with('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

pub fn to_start(entries: &[Entry], login: &[crate::Launch]) -> Vec<Entry> {
    let taken: Vec<String> = login
        .iter()
        .filter_map(|item| match (item.command(), item.app()) {
            (Some(command), _) => program_of(command),
            (None, Some(app)) => Some(app.to_string()),
            (None, None) => None,
        })
        .collect();
    entries
        .iter()
        .filter(|entry| entry.on)
        .filter(|entry| {
            !taken.contains(&entry.id)
                && entry
                    .program()
                    .is_none_or(|program| !taken.contains(&program))
        })
        .cloned()
        .collect()
}

pub fn with_hidden(text: &str, hidden: bool) -> String {
    let wanted = format!("Hidden={hidden}");
    let mut out = Vec::new();
    let mut inside = false;
    let mut placed = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if inside && !placed {
                out.push(wanted.clone());
                placed = true;
            }
            inside = trimmed == "[Desktop Entry]";
            out.push(line.to_string());
            continue;
        }
        if inside
            && trimmed
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == "Hidden")
        {
            if !placed {
                out.push(wanted.clone());
                placed = true;
            }
            continue;
        }
        if inside
            && trimmed
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == "X-GNOME-Autostart-enabled")
        {
            continue;
        }
        out.push(line.to_string());
    }
    if !placed {
        if !inside {
            out.push("[Desktop Entry]".to_string());
        }
        out.push(wanted);
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    joined
}

pub fn set_on(user: &Path, system: &[PathBuf], id: &str, on: bool) -> std::io::Result<()> {
    let name = format!("{id}.desktop");
    let mine = user.join(&name);
    let theirs = system
        .iter()
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file());
    if mine.is_file() {
        let text = std::fs::read_to_string(&mine)?;
        if on
            && let Some(theirs) = &theirs
            && std::fs::read_to_string(theirs).is_ok_and(|base| with_hidden(&base, true) == text)
        {
            return std::fs::remove_file(&mine);
        }
        return std::fs::write(&mine, with_hidden(&text, !on));
    }
    let Some(theirs) = theirs else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no autostart file is called {name}"),
        ));
    };
    let base = std::fs::read_to_string(&theirs)?;
    std::fs::create_dir_all(user)?;
    std::fs::write(&mine, with_hidden(&base, !on))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here() -> Vec<String> {
        vec!["drifter".to_string()]
    }

    fn entry(text: &str) -> Option<Entry> {
        read("x", text, &here(), |_| true)
    }

    const NM: &str =
        "[Desktop Entry]\nName=Network\nExec=nm-applet\nType=Application\nNotShowIn=KDE;GNOME;\n";

    #[test]
    fn a_plain_entry_reads() {
        let found = entry(NM).unwrap();
        assert_eq!(found.name, "Network");
        assert_eq!(found.exec, "nm-applet");
        assert!(found.on);
    }

    #[test]
    fn nm_applet_gets_the_indicator_switch_so_its_icon_reaches_the_tray() {
        let found = entry(NM).unwrap();
        assert_eq!(found.argv().unwrap(), ["nm-applet", "--indicator"]);
        let already = entry(&NM.replace("Exec=nm-applet", "Exec=nm-applet --indicator")).unwrap();
        assert_eq!(already.argv().unwrap(), ["nm-applet", "--indicator"]);
        let other = entry(&NM.replace("Exec=nm-applet", "Exec=blueman-applet")).unwrap();
        assert_eq!(other.argv().unwrap(), ["blueman-applet"]);
    }

    #[test]
    fn only_show_in_and_not_show_in_follow_the_desktop() {
        let gnome = "[Desktop Entry]\nName=G\nExec=g\nOnlyShowIn=GNOME;\n";
        assert_eq!(entry(gnome), None);
        let ours = "[Desktop Entry]\nName=D\nExec=d\nOnlyShowIn=GNOME;Drifter;\n";
        assert!(entry(ours).is_some());
        let not_ours = "[Desktop Entry]\nName=D\nExec=d\nNotShowIn=drifter;\n";
        assert_eq!(entry(not_ours), None);
    }

    #[test]
    fn hidden_and_gnome_disabled_entries_are_listed_but_off() {
        let hidden = entry(&format!("{NM}Hidden=true\n")).unwrap();
        assert!(!hidden.on);
        let disabled = entry(&format!("{NM}X-GNOME-Autostart-enabled=false\n")).unwrap();
        assert!(!disabled.on);
    }

    #[test]
    fn a_missing_try_exec_or_exec_drops_the_entry() {
        let text = "[Desktop Entry]\nName=T\nExec=t\nTryExec=not-here\n";
        assert_eq!(
            read("t", text, &here(), |program| program != "not-here"),
            None
        );
        assert_eq!(entry("[Desktop Entry]\nName=T\n"), None);
        assert_eq!(entry("[Desktop Entry]\nName=L\nExec=l\nType=Link\n"), None);
    }

    #[test]
    fn jobs_drifter_does_itself_are_left_out() {
        for exec in [
            "lxpolkit",
            "/usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1",
            "lxqt-policykit-agent",
            "dunst",
            "xscreensaver -no-splash",
        ] {
            let text = format!("[Desktop Entry]\nName=A\nExec={exec}\n");
            assert_eq!(entry(&text), None, "{exec}");
        }
    }

    #[test]
    fn entries_that_systemd_starts_are_left_out() {
        let text =
            "[Desktop Entry]\nName=P\nExec=start-pulseaudio-x11\nX-GNOME-HiddenUnderSystemd=true\n";
        assert_eq!(entry(text), None);
        let plain = "[Desktop Entry]\nName=P\nExec=p\nX-GNOME-HiddenUnderSystemd=false\n";
        assert!(entry(plain).is_some());
    }

    #[test]
    fn keys_in_other_groups_do_not_count() {
        let text = "[Desktop Entry]\nName=A\nExec=a\n[Desktop Action new]\nName=New\nExec=a --new\nHidden=true\n";
        let found = entry(text).unwrap();
        assert_eq!(
            (found.name.as_str(), found.exec.as_str(), found.on),
            ("A", "a", true)
        );
    }

    #[test]
    fn hiding_rewrites_the_key_in_its_group_only() {
        let text = "[Desktop Entry]\nName=A\nExec=a\nHidden=false\n[Desktop Action x]\nName=X\n";
        let hidden = with_hidden(text, true);
        assert_eq!(
            hidden,
            "[Desktop Entry]\nName=A\nExec=a\nHidden=true\n[Desktop Action x]\nName=X\n"
        );
        assert!(!entry(&hidden).unwrap().on);
        let added = with_hidden(
            "[Desktop Entry]\nName=A\nExec=a\n[Desktop Action x]\n",
            true,
        );
        assert_eq!(
            added,
            "[Desktop Entry]\nName=A\nExec=a\nHidden=true\n[Desktop Action x]\n"
        );
        let shown = with_hidden(&format!("{NM}X-GNOME-Autostart-enabled=false\n"), false);
        assert!(entry(&shown).unwrap().on);
    }

    #[test]
    fn login_items_win_over_autostart_for_the_same_program() {
        let entries = vec![
            entry(NM).unwrap(),
            Entry {
                id: "blueman".into(),
                name: "Blueman".into(),
                exec: "/usr/bin/blueman-applet".into(),
                icon: None,
                on: true,
            },
            Entry {
                id: "off".into(),
                name: "Off".into(),
                exec: "off".into(),
                icon: None,
                on: false,
            },
        ];
        let login = vec![crate::Launch::Full {
            app: None,
            command: Some("blueman-applet --x".into()),
            icon: None,
        }];
        let started: Vec<String> = to_start(&entries, &login)
            .into_iter()
            .map(|e| e.exec)
            .collect();
        assert_eq!(started, ["nm-applet"]);
    }

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("drifter-autostart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_user_folder_overrides_the_system_folder_by_file_name() {
        let root = folder("scan");
        let (user, system) = (root.join("user"), root.join("system"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("nm-applet.desktop"), NM).unwrap();
        std::fs::write(
            system.join("zz.desktop"),
            "[Desktop Entry]\nName=Zed\nExec=zz\n",
        )
        .unwrap();
        std::fs::write(user.join("nm-applet.desktop"), with_hidden(NM, true)).unwrap();
        std::fs::write(user.join("notes.txt"), "not an entry").unwrap();
        let found = scan(&user, std::slice::from_ref(&system), &here(), |_| true);
        let seen: Vec<(&str, bool)> = found.iter().map(|e| (e.id.as_str(), e.on)).collect();
        assert_eq!(seen, [("nm-applet", false), ("zz", true)]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn turning_a_system_entry_off_and_on_leaves_no_file_behind() {
        let root = folder("toggle");
        let (user, system) = (root.join("user"), root.join("system"));
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("nm-applet.desktop"), NM).unwrap();
        let dirs = vec![system.clone()];

        set_on(&user, &dirs, "nm-applet", false).unwrap();
        assert!(!scan(&user, &dirs, &here(), |_| true)[0].on);
        set_on(&user, &dirs, "nm-applet", true).unwrap();
        assert!(!user.join("nm-applet.desktop").exists());
        assert!(scan(&user, &dirs, &here(), |_| true)[0].on);
        assert!(set_on(&user, &dirs, "gone", false).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_users_own_entry_is_edited_in_place() {
        let root = folder("own");
        let user = root.join("user");
        std::fs::create_dir_all(&user).unwrap();
        let own = "[Desktop Entry]\nName=Mine\nExec=mine\n";
        std::fs::write(user.join("mine.desktop"), own).unwrap();
        set_on(&user, &[], "mine", false).unwrap();
        assert!(!scan(&user, &[], &here(), |_| true)[0].on);
        set_on(&user, &[], "mine", true).unwrap();
        assert!(user.join("mine.desktop").exists());
        assert!(scan(&user, &[], &here(), |_| true)[0].on);
        let _ = std::fs::remove_dir_all(&root);
    }
}
