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
