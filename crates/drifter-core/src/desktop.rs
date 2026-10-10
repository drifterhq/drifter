use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub words: String,
    pub categories: Vec<String>,
    pub terminal: bool,
}

pub fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/applications"));
    }
    roots.push(PathBuf::from("/usr/local/share/applications"));
    roots.push(PathBuf::from("/usr/share/applications"));
    roots
}

pub fn read(id: &str, text: &str) -> Option<App> {
    let mut inside = false;
    let mut name = None;
    let mut icon = None;
    let mut exec = None;
    let mut kind = None;
    let mut hidden = false;
    let mut terminal = false;
    let mut words: Vec<String> = Vec::new();
    let mut categories: Vec<String> = Vec::new();
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
            "Name" if name.is_none() => name = Some(value),
            "Icon" if icon.is_none() => icon = Some(value),
            "Exec" if exec.is_none() => exec = Some(value),
            "Type" => kind = Some(value),
            "Terminal" => terminal = value == "true",
            "Categories" => {
                categories.extend(
                    value
                        .split(';')
                        .filter(|word| !word.is_empty())
                        .map(str::to_string),
                );
                words.extend(
                    value
                        .split(';')
                        .filter(|word| !word.is_empty())
                        .map(str::to_lowercase),
                );
            }
            "GenericName" | "Keywords" => {
                words.extend(
                    value
                        .split([';', ' ', ','])
                        .filter(|word| !word.is_empty())
                        .map(str::to_lowercase),
                );
            }
            "NoDisplay" | "Hidden" if value == "true" => hidden = true,
            _ => {}
        }
    }
    if hidden || kind.as_deref() != Some("Application") {
        return None;
    }
    Some(App {
        id: id.to_string(),
        name: name.unwrap_or_else(|| id.to_string()),
        icon,
        exec,
        words: words.join(" "),
        categories,
        terminal,
    })
}

pub fn installed_in(roots: &[PathBuf]) -> Vec<App> {
    let mut found: Vec<App> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = desktop_id(&path) else {
                continue;
            };
            if found.iter().any(|app| app.id == id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = read(&id, &text) {
                found.push(app);
            }
        }
    }
    found.sort_by_key(|app| app.name.to_lowercase());
    found
}

pub const GROUPS: [(&str, &[&str]); 10] = [
    ("Internet", &["Network", "WebBrowser", "Email", "Chat"]),
    (
        "Office",
        &["Office", "WordProcessor", "Spreadsheet", "Presentation"],
    ),
    ("Graphics", &["Graphics", "Photography", "Viewer"]),
    ("Sound & Video", &["AudioVideo", "Audio", "Video", "Music"]),
    ("Development", &["Development", "IDE"]),
    ("Games", &["Game"]),
    ("Education", &["Education", "Science"]),
    (
        "System",
        &[
            "System",
            "Settings",
            "Monitor",
            "TerminalEmulator",
            "FileManager",
            "PackageManager",
        ],
    ),
    (
        "Accessories",
        &[
            "Utility",
            "Accessibility",
            "TextEditor",
            "Calculator",
            "Archiving",
        ],
    ),
    ("Other", &[]),
];

pub fn group_of(app: &App) -> &'static str {
    GROUPS
        .iter()
        .find(|(_, tags)| {
            tags.iter()
                .any(|tag| app.categories.iter().any(|held| held == tag))
        })
        .map(|(name, _)| *name)
        .unwrap_or("Other")
}

pub fn grouped(apps: &[App]) -> Vec<(&'static str, Vec<App>)> {
    let mut out: Vec<(&'static str, Vec<App>)> = Vec::new();
    for (name, _) in GROUPS {
        let mut members: Vec<App> = apps
            .iter()
            .filter(|app| group_of(app) == name)
            .cloned()
            .collect();
        members.sort_by_key(|app| app.name.to_lowercase());
        if !members.is_empty() {
            out.push((name, members));
        }
    }
    out
}

pub fn installed() -> Vec<App> {
    let mut apps = installed_in(&roots());
    if let Some(own) = crate::activity::own_app()
        && !apps.iter().any(|app| app.id == own.id)
    {
        apps.push(own);
        apps.sort_by_key(|app| app.name.to_lowercase());
    }
    apps
}

fn desktop_id(path: &Path) -> Option<String> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
        return None;
    }
    path.file_stem()?.to_str().map(str::to_string)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Files,
    Terminal,
    Browser,
    Editor,
    Mail,
    Images,
    Video,
    Music,
}

impl Role {
    pub const ALL: [Role; 8] = [
        Role::Browser,
        Role::Mail,
        Role::Files,
        Role::Terminal,
        Role::Editor,
        Role::Images,
        Role::Video,
        Role::Music,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Role::Files => "files",
            Role::Terminal => "terminal",
            Role::Browser => "browser",
            Role::Editor => "editor",
            Role::Mail => "mail",
            Role::Images => "images",
            Role::Video => "video",
            Role::Music => "music",
        }
    }

    pub fn from_key(word: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|role| role.key() == word)
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Files => "File manager",
            Role::Terminal => "Terminal",
            Role::Browser => "Web browser",
            Role::Editor => "Text editor",
            Role::Mail => "Mail",
            Role::Images => "Photos",
            Role::Video => "Video",
            Role::Music => "Music",
        }
    }

    pub fn mimes(self) -> &'static [&'static str] {
        match self {
            Role::Files => &["inode/directory"],
            Role::Terminal => &["x-scheme-handler/terminal"],
            Role::Browser => &[
                "x-scheme-handler/http",
                "x-scheme-handler/https",
                "text/html",
            ],
            Role::Editor => &["text/plain"],
            Role::Mail => &["x-scheme-handler/mailto"],
            Role::Images => &["image/png", "image/jpeg", "image/gif", "image/webp"],
            Role::Video => &["video/mp4", "video/x-matroska", "video/webm"],
            Role::Music => &["audio/mpeg", "audio/flac", "audio/ogg"],
        }
    }

    fn serves(self, text: &str) -> bool {
        match self {
            Role::Terminal => text
                .lines()
                .any(|line| line.starts_with("Categories=") && line.contains("TerminalEmulator")),
            Role::Editor => {
                let graphical = !text.lines().any(|line| line.trim() == "Terminal=true");
                graphical
                    && text
                        .lines()
                        .any(|line| line.starts_with("Categories=") && line.contains("TextEditor"))
            }
            _ => text
                .lines()
                .any(|line| line.starts_with("MimeType=") && line.contains(self.mimes()[0])),
        }
    }
}

pub fn mime_lists() -> Vec<PathBuf> {
    let mut lists = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        lists.push(home.join(".config/mimeapps.list"));
        lists.push(home.join(".local/share/applications/mimeapps.list"));
    }
    lists.push(PathBuf::from("/usr/share/applications/mimeapps.list"));
    lists.push(PathBuf::from("/usr/share/applications/defaults.list"));
    lists
}

pub fn chosen_in(text: &str, mime: &str) -> Vec<String> {
    let mut out = listed(text, mime, "[Default Applications]");
    out.extend(listed(text, mime, "[Added Associations]"));
    out
}

fn listed(text: &str, mime: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == section;
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| inside) else {
            continue;
        };
        if key.trim() != mime {
            continue;
        }
        out.extend(
            value
                .split(';')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| part.trim_end_matches(".desktop").to_string()),
        );
    }
    out
}

fn installed_id(roots: &[PathBuf], id: &str) -> bool {
    roots
        .iter()
        .any(|root| root.join(format!("{id}.desktop")).is_file())
}

pub fn system_default_in(role: Role, lists: &[PathBuf], roots: &[PathBuf]) -> Option<String> {
    for list in lists {
        let Ok(text) = std::fs::read_to_string(list) else {
            continue;
        };
        for mime in role.mimes() {
            if let Some(id) = chosen_in(&text, mime)
                .into_iter()
                .find(|id| installed_id(roots, id))
            {
                return Some(id);
            }
        }
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut found: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let id = desktop_id(&path)?;
                let text = std::fs::read_to_string(&path).ok()?;
                (read(&id, &text).is_some() && role.serves(&text)).then_some(id)
            })
            .collect();
        found.sort();
        if let Some(first) = found.into_iter().next() {
            return Some(first);
        }
    }
    None
}

pub fn serving_in(role: Role, roots: &[PathBuf]) -> Vec<App> {
    let mut found: Vec<App> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = desktop_id(&path) else {
                continue;
            };
            if found.iter().any(|app| app.id == id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !role.serves(&text) {
                continue;
            }
            if let Some(app) = read(&id, &text) {
                found.push(app);
            }
        }
    }
    let ids: Vec<String> = found.iter().map(|app| app.id.to_lowercase()).collect();
    found.retain(|app| {
        let id = app.id.to_lowercase();
        let side = ["client", "server"].iter().find_map(|tail| id.strip_suffix(tail));
        let Some(stem) = side.map(|stem| stem.trim_end_matches(['-', '_', '.'])) else {
            return true;
        };
        !ids.iter().any(|other| other != &id && other == stem)
    });
    found.sort_by_key(|app| app.name.to_lowercase());
    found
}

pub fn serving(role: Role) -> Vec<App> {
    serving_in(role, &roots())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub role: Role,
    pub name: &'static str,
    pub ids: &'static [&'static str],
    pub apt: &'static str,
    pub dnf: &'static str,
}

pub const OFFERS: [Offer; 16] = [
    Offer { role: Role::Mail, name: "Thunderbird", ids: &["thunderbird", "org.mozilla.Thunderbird", "net.thunderbird.Thunderbird"], apt: "thunderbird", dnf: "thunderbird" },
    Offer { role: Role::Mail, name: "Geary", ids: &["org.gnome.Geary", "geary"], apt: "geary", dnf: "geary" },
    Offer { role: Role::Images, name: "Image Viewer", ids: &["org.gnome.Loupe", "loupe", "org.gnome.eog", "eog"], apt: "eog", dnf: "loupe" },
    Offer { role: Role::Images, name: "Ristretto", ids: &["org.xfce.ristretto", "ristretto"], apt: "ristretto", dnf: "ristretto" },
    Offer { role: Role::Video, name: "mpv", ids: &["mpv", "io.mpv.Mpv"], apt: "mpv", dnf: "mpv" },
    Offer { role: Role::Video, name: "VLC", ids: &["vlc", "org.videolan.VLC"], apt: "vlc", dnf: "vlc" },
    Offer { role: Role::Music, name: "Rhythmbox", ids: &["org.gnome.Rhythmbox3", "rhythmbox"], apt: "rhythmbox", dnf: "rhythmbox" },
    Offer { role: Role::Music, name: "Lollypop", ids: &["org.gnome.Lollypop", "lollypop"], apt: "lollypop", dnf: "lollypop" },
    Offer { role: Role::Browser, name: "Firefox", ids: &["firefox-esr", "firefox", "org.mozilla.firefox"], apt: "firefox-esr", dnf: "firefox" },
    Offer { role: Role::Browser, name: "Chromium", ids: &["chromium", "chromium-browser", "org.chromium.Chromium"], apt: "chromium", dnf: "chromium" },
    Offer { role: Role::Terminal, name: "Foot", ids: &["foot", "org.codeberg.dnkl.foot"], apt: "foot", dnf: "foot" },
    Offer { role: Role::Terminal, name: "Kitty", ids: &["kitty"], apt: "kitty", dnf: "kitty" },
    Offer { role: Role::Files, name: "Thunar", ids: &["thunar", "Thunar"], apt: "thunar", dnf: "Thunar" },
    Offer { role: Role::Files, name: "Files", ids: &["org.gnome.Nautilus", "nautilus"], apt: "nautilus", dnf: "nautilus" },
    Offer { role: Role::Editor, name: "Mousepad", ids: &["org.xfce.mousepad", "mousepad"], apt: "mousepad", dnf: "mousepad" },
    Offer { role: Role::Editor, name: "Text Editor", ids: &["org.gnome.TextEditor", "gnome-text-editor"], apt: "gnome-text-editor", dnf: "gnome-text-editor" },
];

pub fn offers(role: Role, have: &[App]) -> Vec<Offer> {
    OFFERS
        .iter()
        .filter(|offer| offer.role == role)
        .filter(|offer| !have.iter().any(|app| offer.ids.iter().any(|id| id.eq_ignore_ascii_case(&app.id))))
        .copied()
        .collect()
}

pub fn system_default(role: Role) -> Option<String> {
    system_default_in(role, &mime_lists(), &roots())
}

pub fn clear_default(text: &str, role: Role) -> String {
    let mut inside = false;
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[Default Applications]";
        }
        let key = trimmed.split_once('=').map(|(key, _)| key.trim());
        if inside && key.is_some_and(|key| role.mimes().contains(&key)) {
            continue;
        }
        out.push(line);
    }
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

pub fn user_mime_list() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
        .join("mimeapps.list")
}

pub fn set_default(text: &str, role: Role, id: &str) -> String {
    set_defaults(text, role.mimes(), id)
}

pub fn set_defaults(text: &str, mimes: &[&str], id: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let header = "[Default Applications]";
    let start = match lines.iter().position(|line| line.trim() == header) {
        Some(at) => at,
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(header.to_string());
            lines.len() - 1
        }
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim().starts_with('['))
        .map(|at| start + 1 + at)
        .unwrap_or(lines.len());
    let mut kept: Vec<String> = lines[start + 1..end]
        .iter()
        .filter(|line| {
            let key = line.split_once('=').map(|(key, _)| key.trim());
            !key.is_some_and(|key| mimes.contains(&key))
        })
        .cloned()
        .collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    for mime in mimes {
        kept.push(format!("{mime}={id}.desktop;"));
    }
    let mut out: Vec<String> = lines[..=start].to_vec();
    out.extend(kept);
    if end < lines.len() {
        out.push(String::new());
        out.extend(lines[end..].iter().cloned());
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_fall_into_their_first_matching_group() {
        let app = |id: &str, cats: &str| {
            read(
                id,
                &format!("[Desktop Entry]\nType=Application\nName={id}\nCategories={cats}\n"),
            )
            .unwrap()
        };
        let apps = vec![
            app("firefox", "Network;WebBrowser;"),
            app("foot", "System;TerminalEmulator;"),
            app("gimp", "Graphics;2DGraphics;"),
            app("odd", "Weird;"),
            app("thunderbird", "Network;Email;"),
        ];
        let groups = grouped(&apps);
        assert_eq!(groups[0].0, "Internet");
        assert_eq!(
            groups[0]
                .1
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>(),
            ["firefox", "thunderbird"]
        );
        assert_eq!(group_of(&apps[1]), "System");
        assert_eq!(groups.last().unwrap().0, "Other");
        assert!(groups.iter().all(|(_, members)| !members.is_empty()));
    }

    const FOOT: &str = "[Desktop Entry]\nType=Application\nName=Foot\nName[de]=Fuß\nIcon=foot\nExec=foot\n\n[Desktop Action new]\nName=New window\nExec=foot --new\n";

    #[test]
    fn an_app_reads_its_name_icon_and_command() {
        let app = read("foot", FOOT).unwrap();
        assert_eq!(app.name, "Foot");
        assert_eq!(app.icon.as_deref(), Some("foot"));
        assert_eq!(app.exec.as_deref(), Some("foot"));
    }

    #[test]
    fn hidden_entries_and_links_are_not_apps() {
        assert!(
            read(
                "x",
                "[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n"
            )
            .is_none()
        );
        assert!(read("x", "[Desktop Entry]\nType=Link\nName=X\n").is_none());
    }

    #[test]
    fn an_app_with_no_name_is_named_by_its_file() {
        let app = read("tool", "[Desktop Entry]\nType=Application\n").unwrap();
        assert_eq!(app.name, "tool");
    }

    #[test]
    fn a_default_is_read_from_the_mime_list() {
        let text = "[Added Associations]\ntext/plain=gedit.desktop;\n[Default Applications]\ninode/directory=pcmanfm.desktop;thunar.desktop\n";
        assert_eq!(chosen_in(text, "inode/directory"), ["pcmanfm", "thunar"]);
        assert!(chosen_in(text, "x-scheme-handler/http").is_empty());
    }

    #[test]
    fn setting_a_default_replaces_only_its_own_lines() {
        let text = "[Added Associations]\ntext/plain=gedit.desktop;\n\n[Default Applications]\ntext/html=old.desktop;\nimage/png=eog.desktop;\n";
        let out = set_default(text, Role::Browser, "firefox-esr");
        assert!(out.contains("image/png=eog.desktop;"), "{out}");
        assert!(out.contains("text/plain=gedit.desktop;"), "{out}");
        assert!(!out.contains("old.desktop"), "{out}");
        assert_eq!(chosen_in(&out, "x-scheme-handler/https"), ["firefox-esr"]);
        assert_eq!(chosen_in(&out, "text/html"), ["firefox-esr"]);
    }

    #[test]
    fn the_default_section_wins_over_added_associations_in_any_order() {
        let text = "[Added Associations]\nx-scheme-handler/http=chromium.desktop;\n[Default Applications]\nx-scheme-handler/http=epiphany.desktop;\n";
        assert_eq!(
            chosen_in(text, "x-scheme-handler/http"),
            ["epiphany", "chromium"]
        );
    }

    #[test]
    fn clearing_a_default_removes_only_its_own_lines() {
        let set = set_default(
            "[Default Applications]\nimage/png=eog.desktop;\n",
            Role::Browser,
            "firefox-esr",
        );
        let cleared = clear_default(&set, Role::Browser);
        assert!(chosen_in(&cleared, "text/html").is_empty(), "{cleared}");
        assert_eq!(chosen_in(&cleared, "image/png"), ["eog"]);
    }

    #[test]
    fn setting_a_default_makes_the_section_when_there_is_none() {
        let out = set_default("", Role::Terminal, "foot");
        assert_eq!(chosen_in(&out, "x-scheme-handler/terminal"), ["foot"]);
        let again = set_default(&out, Role::Files, "pcmanfm");
        assert_eq!(chosen_in(&again, "x-scheme-handler/terminal"), ["foot"]);
        assert_eq!(chosen_in(&again, "inode/directory"), ["pcmanfm"]);
        assert_eq!(again.matches("[Default Applications]").count(), 1);
    }

    #[test]
    fn the_system_default_prefers_the_list_then_a_serving_app() {
        let base = std::env::temp_dir().join(format!("drifter-roles-{}", std::process::id()));
        let apps = base.join("apps");
        std::fs::create_dir_all(&apps).unwrap();
        let entry =
            |mime: &str| format!("[Desktop Entry]\nType=Application\nName=X\nMimeType={mime};\n");
        std::fs::write(apps.join("zfiles.desktop"), entry("inode/directory")).unwrap();
        std::fs::write(apps.join("afiles.desktop"), entry("inode/directory")).unwrap();
        std::fs::write(
            apps.join("term.desktop"),
            "[Desktop Entry]\nType=Application\nName=T\nCategories=System;TerminalEmulator;\n",
        )
        .unwrap();
        let list = base.join("mimeapps.list");
        std::fs::write(
            &list,
            "[Default Applications]\ninode/directory=gone.desktop;zfiles.desktop;\n",
        )
        .unwrap();
        let roots = [apps.clone()];
        assert_eq!(
            system_default_in(Role::Files, std::slice::from_ref(&list), &roots).as_deref(),
            Some("zfiles")
        );
        assert_eq!(
            system_default_in(Role::Files, &[], &roots).as_deref(),
            Some("afiles")
        );
        assert_eq!(
            system_default_in(Role::Terminal, &[], &roots).as_deref(),
            Some("term")
        );
        assert_eq!(system_default_in(Role::Browser, &[], &roots), None);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn serving_apps_skip_client_and_server_twins_and_terminal_editors() {
        let base = std::env::temp_dir().join(format!("drifter-serving-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let term = |name: &str| format!("[Desktop Entry]\nType=Application\nName={name}\nCategories=System;TerminalEmulator;\n");
        std::fs::write(base.join("foot.desktop"), term("Foot")).unwrap();
        std::fs::write(base.join("footclient.desktop"), term("Foot Client")).unwrap();
        std::fs::write(base.join("foot-server.desktop"), term("Foot Server")).unwrap();
        std::fs::write(base.join("kitty.desktop"), term("kitty")).unwrap();
        std::fs::write(
            base.join("vim.desktop"),
            "[Desktop Entry]\nType=Application\nName=Vim\nTerminal=true\nCategories=Utility;TextEditor;\n",
        )
        .unwrap();
        std::fs::write(
            base.join("org.xfce.mousepad.desktop"),
            "[Desktop Entry]\nType=Application\nName=Mousepad\nCategories=Utility;TextEditor;\n",
        )
        .unwrap();
        let roots = [base.clone()];
        let terms: Vec<String> = serving_in(Role::Terminal, &roots).into_iter().map(|app| app.id).collect();
        let editors: Vec<String> = serving_in(Role::Editor, &roots).into_iter().map(|app| app.id).collect();
        std::fs::remove_dir_all(&base).unwrap();
        assert_eq!(terms, ["foot", "kitty"]);
        assert_eq!(editors, ["org.xfce.mousepad"]);
        let have = vec![read("org.xfce.mousepad", "[Desktop Entry]\nType=Application\nName=Mousepad\n").unwrap()];
        let left: Vec<&str> = offers(Role::Editor, &have).iter().map(|offer| offer.name).collect();
        assert_eq!(left, ["Text Editor"]);
    }

    #[test]
    fn the_first_root_wins_and_the_list_is_in_name_order() {
        let base = std::env::temp_dir().join(format!("drifter-apps-{}", std::process::id()));
        let mine = base.join("mine");
        let system = base.join("system");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(
            mine.join("foot.desktop"),
            FOOT.replace("Name=Foot", "Name=My Foot"),
        )
        .unwrap();
        std::fs::write(system.join("foot.desktop"), FOOT).unwrap();
        std::fs::write(
            system.join("alpha.desktop"),
            "[Desktop Entry]\nType=Application\nName=alpha\n",
        )
        .unwrap();
        std::fs::write(system.join("notes.txt"), "x").unwrap();
        let apps = installed_in(&[mine, system]);
        std::fs::remove_dir_all(&base).unwrap();
        let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["alpha", "My Foot"]);
    }
}
