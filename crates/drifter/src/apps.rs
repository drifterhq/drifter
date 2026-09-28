use std::path::{Path, PathBuf};

pub struct Entry {
    pub icon: Option<PathBuf>,
    pub exec: Option<String>,
}

pub fn find(app_id: &str) -> Option<Entry> {
    let file = desktop_file(app_id)?;
    let text = std::fs::read_to_string(&file).ok()?;
    let mut icon = None;
    let mut exec = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Icon=")
            && icon.is_none()
        {
            icon = icon_file(rest);
        }
        if let Some(rest) = line.strip_prefix("Exec=")
            && exec.is_none()
        {
            exec = Some(clean_exec(rest));
        }
    }

    Some(Entry { icon, exec })
}

fn desktop_file(app_id: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/applications"));
    }
    roots.push(PathBuf::from("/usr/share/applications"));
    roots.push(PathBuf::from("/usr/local/share/applications"));

    let names = [
        format!("{app_id}.desktop"),
        format!("{}.desktop", app_id.to_lowercase()),
        format!("org.{app_id}.desktop"),
    ];

    for root in roots {
        for name in &names {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn icon_file(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let direct = Path::new(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct.to_path_buf());
    }

    let mut roots = Vec::new();
    for size in ["64x64", "48x48", "128x128", "32x32", "256x256"] {
        roots.push(
            PathBuf::from("/usr/share/icons/hicolor")
                .join(size)
                .join("apps"),
        );
    }
    roots.push(PathBuf::from("/usr/share/pixmaps"));

    roots
        .into_iter()
        .map(|root| root.join(format!("{name}.png")))
        .find(|candidate| candidate.is_file())
}

fn clean_exec(line: &str) -> String {
    line.split_whitespace()
        .filter(|word| !word.starts_with('%'))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn launch(command: &str, socket: &str) {
    let mut words = command.split_whitespace();
    let Some(program) = words.next() else {
        return;
    };
    let result = std::process::Command::new(program)
        .args(words)
        .env("WAYLAND_DISPLAY", socket)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(error) = result {
        eprintln!("drifter: cannot start {program}: {error}");
    }
}

pub fn file_manager() -> Option<String> {
    handler("inode/directory")
}

pub fn open_folder(path: &str, socket: &str) {
    if let Some(files) = file_manager()
        && let Some(exec) = find(&files).and_then(|entry| entry.exec)
    {
        launch(&format!("{exec} {path}"), socket);
        return;
    }
    if PathBuf::from("/usr/bin/xdg-open").is_file() {
        launch(&format!("xdg-open {path}"), socket);
        return;
    }
    eprintln!("drifter: no file manager is installed, so {path} cannot be opened");
}

pub fn defaults() -> Vec<String> {
    let mut chosen = Vec::new();
    for mime in ["x-scheme-handler/http", "inode/directory"] {
        if let Some(id) = handler(mime) {
            chosen.push(id);
        }
    }
    if let Some(id) = terminal() {
        chosen.push(id);
    }
    chosen.dedup();
    chosen
}

fn handler(mime: &str) -> Option<String> {
    for list in mime_lists() {
        let Ok(text) = std::fs::read_to_string(&list) else {
            continue;
        };
        let mut inside = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                inside = line == "[Default Applications]" || line == "[Added Associations]";
                continue;
            }
            if !inside {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() != mime {
                continue;
            }
            for entry in value.split(';').filter(|part| !part.trim().is_empty()) {
                let id = entry.trim().trim_end_matches(".desktop").to_string();
                if find(&id).is_some() {
                    return Some(id);
                }
            }
        }
    }
    None
}

fn mime_lists() -> Vec<PathBuf> {
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

fn terminal() -> Option<String> {
    if let Some(id) = handler("x-scheme-handler/terminal") {
        return Some(id);
    }

    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/applications"));
    }
    roots.push(PathBuf::from("/usr/share/applications"));

    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut found: Vec<String> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let terminal = text
                .lines()
                .any(|line| line.starts_with("Categories=") && line.contains("TerminalEmulator"));
            let hidden = text
                .lines()
                .any(|line| line.trim() == "NoDisplay=true" || line.trim() == "Hidden=true");
            if terminal
                && !hidden
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                found.push(stem.to_string());
            }
        }
        found.sort();
        if let Some(first) = found.into_iter().next() {
            return Some(first);
        }
    }
    None
}
