use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub exec: Option<String>,
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

pub fn installed() -> Vec<App> {
    installed_in(&roots())
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
}

impl Role {
    pub const ALL: [Role; 3] = [Role::Files, Role::Terminal, Role::Browser];

    pub fn label(self) -> &'static str {
        match self {
            Role::Files => "File manager",
            Role::Terminal => "Terminal",
            Role::Browser => "Web browser",
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
        }
    }

    fn serves(self, text: &str) -> bool {
        match self {
            Role::Terminal => text
                .lines()
                .any(|line| line.starts_with("Categories=") && line.contains("TerminalEmulator")),
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

pub fn set_default(text: &str, role: Role, id: &str) -> String {
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
            !key.is_some_and(|key| role.mimes().contains(&key))
        })
        .cloned()
        .collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    for mime in role.mimes() {
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
