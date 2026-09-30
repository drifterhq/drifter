use std::path::PathBuf;

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
            exec = Some(rest.to_string());
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
    drifter_core::icon::find(name, None, 64)
}

pub fn run(item: &drifter_core::Launch, socket: &str) {
    if let Some(command) = item.command() {
        match drifter_core::exec::words(command) {
            Some(argv) if !argv.is_empty() => launch(&argv, socket),
            _ => eprintln!("drifter: cannot read the command line {command:?}"),
        }
        return;
    }
    let Some(app) = item.app() else {
        return;
    };
    match find(app).and_then(|entry| entry.exec) {
        Some(exec) => start(&exec, &[], socket),
        None => eprintln!("drifter: no installed app is called {app}"),
    }
}

pub fn start(exec: &str, files: &[&str], socket: &str) {
    match drifter_core::exec::command(exec, files) {
        Some(argv) => launch(&argv, socket),
        None => eprintln!("drifter: cannot read the command line {exec:?}"),
    }
}

static X11: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|dir| PathBuf::from(dir).join(program))
        .find(|path| path.is_file())
}

fn free_display() -> Option<u32> {
    (0..64).find(|number| {
        !PathBuf::from(format!("/tmp/.X11-unix/X{number}")).exists()
            && !PathBuf::from(format!("/tmp/.X{number}-lock")).exists()
    })
}

pub fn start_x11(socket: &str) {
    let Some(program) = on_path("xwayland-satellite") else {
        eprintln!("drifter: xwayland-satellite is not installed, so X11 apps cannot run");
        return;
    };
    let Some(number) = free_display() else {
        eprintln!("drifter: no free X display number, so X11 apps cannot run");
        return;
    };
    let display = format!(":{number}");
    let result = std::process::Command::new(&program)
        .arg(&display)
        .env("WAYLAND_DISPLAY", socket)
        .env_remove("DISPLAY")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match result {
        Ok(mut child) => {
            println!("x11         {display} through {}", program.display());
            let _ = X11.set(display);
            std::thread::spawn(move || {
                let _ = child.wait();
                eprintln!("drifter: xwayland-satellite stopped");
            });
        }
        Err(error) => eprintln!("drifter: cannot start xwayland-satellite: {error}"),
    }
}

pub fn launch(argv: &[String], socket: &str) {
    let Some((program, rest)) = argv.split_first() else {
        return;
    };
    let mut command = std::process::Command::new(program);
    match X11.get() {
        Some(display) => command.env("DISPLAY", display),
        None => command.env_remove("DISPLAY"),
    };
    let result = command
        .args(rest)
        .env("WAYLAND_DISPLAY", socket)
        .env("GTK_CSD", "0")
        .env("QT_WAYLAND_DISABLE_WINDOWDECORATION", "1")
        .env("XDG_SESSION_TYPE", "wayland")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match result {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(error) => eprintln!("drifter: cannot start {program}: {error}"),
    }
}

pub fn file_manager() -> Option<String> {
    handler("inode/directory").or_else(|| serving("inode/directory"))
}

fn serving(mime: &str) -> Option<String> {
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
            let serves = text
                .lines()
                .any(|line| line.starts_with("MimeType=") && line.contains(mime));
            let hidden = text
                .lines()
                .any(|line| line.trim() == "NoDisplay=true" || line.trim() == "Hidden=true");
            if serves
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

pub fn open_folder(path: &str, socket: &str) {
    if let Some(files) = file_manager()
        && let Some(exec) = find(&files).and_then(|entry| entry.exec)
    {
        start(&exec, &[path], socket);
        return;
    }
    if PathBuf::from("/usr/bin/xdg-open").is_file() || PathBuf::from("/bin/xdg-open").is_file() {
        launch(&["xdg-open".to_string(), path.to_string()], socket);
        return;
    }
    eprintln!("drifter: no file manager is installed, so {path} cannot be opened");
}

pub fn browser() -> Option<String> {
    handler("x-scheme-handler/http").or_else(|| serving("x-scheme-handler/http"))
}

pub fn defaults(files: bool, terminal_on: bool, browser_on: bool) -> Vec<String> {
    let mut chosen = Vec::new();
    let wanted = [
        (files, file_manager()),
        (terminal_on, terminal()),
        (browser_on, browser()),
    ];
    for (on, id) in wanted {
        if let (true, Some(id)) = (on, id)
            && !chosen.contains(&id)
        {
            chosen.push(id);
        }
    }
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
