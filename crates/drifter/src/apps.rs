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
