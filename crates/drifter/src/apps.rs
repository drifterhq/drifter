use std::path::PathBuf;

pub struct Entry {
    pub name: Option<String>,
    pub icon: Option<PathBuf>,
    pub exec: Option<String>,
}

pub fn find(app_id: &str) -> Option<Entry> {
    let Some(file) = desktop_file(app_id) else {
        return drifter_core::desktop::own_apps()
            .into_iter()
            .find(|app| app.id == app_id)
            .map(|app| Entry {
                name: Some(app.name),
                icon: app.icon.as_deref().and_then(icon_file),
                exec: app.exec,
            });
    };
    let text = std::fs::read_to_string(&file).ok()?;
    let mut icon = None;
    let mut exec = None;
    let mut name = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Name=")
            && name.is_none()
        {
            name = Some(rest.to_string());
        }
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

    Some(Entry { name, icon, exec })
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
            _ => log!("drifter: cannot read the command line {command:?}"),
        }
        return;
    }
    let Some(app) = item.app() else {
        return;
    };
    match find(app).and_then(|entry| entry.exec) {
        Some(exec) => start(&exec, &[], socket),
        None => log!("drifter: no installed app is called {app}"),
    }
}

pub fn start(exec: &str, files: &[&str], socket: &str) {
    match drifter_core::exec::command(exec, files) {
        Some(argv) => launch(&argv, socket),
        None => log!("drifter: cannot read the command line {exec:?}"),
    }
}

pub fn fresh_signals(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::pthread_sigmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            Ok(())
        })
    }
}

static X11: std::sync::OnceLock<String> = std::sync::OnceLock::new();
static ENV: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

pub fn set_env(key: &str, value: Option<String>) {
    if let Ok(mut held) = ENV.lock() {
        held.retain(|(name, _)| name != key);
        if let Some(value) = value {
            held.push((key.to_string(), value));
        }
    }
}

pub fn open_role(
    role: drifter_core::desktop::Role,
    chosen: &drifter_core::AppsConfig,
    socket: &str,
) {
    let item = chosen
        .get(role)
        .cloned()
        .or_else(|| drifter_core::desktop::system_default(role).map(drifter_core::Launch::App));
    match (role, item) {
        (drifter_core::desktop::Role::Files, item) => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
            open_folder(&home, item.as_ref(), socket);
        }
        (_, Some(item)) => run(&item, socket),
        (role, None) => log!("drifter: no {role:?} app is set or installed"),
    }
}

pub fn x11_display() -> Option<String> {
    X11.get().cloned()
}

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

const SATELLITE_LEAST: (u32, u32, u32) = (0, 8, 3);

fn satellite_version(text: &str) -> Option<(u32, u32, u32)> {
    let word = text.split_whitespace().next()?.trim_start_matches('v');
    let mut parts = word.split(['-', '+']).next()?.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    Some((major, minor, patch))
}

fn check_satellite(program: &std::path::Path) {
    let Ok(output) = std::process::Command::new(program)
        .arg("-version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(found) = satellite_version(&text) else {
        return;
    };
    if found < SATELLITE_LEAST {
        let (major, minor, patch) = SATELLITE_LEAST;
        log!(
            "drifter: {} is version {}; X11 menus (Steam, Chromium apps) close as they open; install xwayland-satellite {major}.{minor}.{patch} or newer (Drifter builds one in its packages, or run scripts/satellite.sh)",
            program.display(),
            text.trim()
        );
    }
}

pub fn start_x11(socket: &str) {
    let Some(program) = on_path("xwayland-satellite") else {
        log!("drifter: xwayland-satellite is not installed, so X11 apps cannot run");
        return;
    };
    let Some(number) = free_display() else {
        log!("drifter: no free X display number, so X11 apps cannot run");
        return;
    };
    let display = format!(":{number}");
    let result = fresh_signals(&mut std::process::Command::new(&program))
        .arg(&display)
        .env("WAYLAND_DISPLAY", socket)
        .env_remove("DISPLAY")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match result {
        Ok(mut child) => {
            out!("x11         {display} through {}", program.display());
            start_xsettings(&display);
            let _ = X11.set(display);
            std::thread::spawn(move || {
                check_satellite(&program);
                let _ = child.wait();
                log!("drifter: xwayland-satellite stopped");
            });
        }
        Err(error) => log!("drifter: cannot start xwayland-satellite: {error}"),
    }
}

static XSETTINGS: std::sync::Mutex<(String, Option<u32>)> = std::sync::Mutex::new((String::new(), None));

fn xsettings_file() -> PathBuf {
    drifter_core::Config::state_dir().join("xsettingsd.conf")
}

pub fn xsettings_text(theme: &str, icons: Option<&str>, cursor: Option<&str>, cursor_size: u32) -> String {
    let mut text = format!("Net/ThemeName \"{theme}\"\n");
    if let Some(icons) = icons {
        text.push_str(&format!("Net/IconThemeName \"{icons}\"\n"));
    }
    if let Some(cursor) = cursor {
        text.push_str(&format!("Gtk/CursorThemeName \"{cursor}\"\n"));
    }
    text.push_str(&format!("Gtk/CursorThemeSize {cursor_size}\n"));
    text
}

fn start_xsettings(display: &str) {
    let Some(program) = on_path("xsettingsd") else {
        log!("drifter: xsettingsd is not installed, so X11 apps keep their own theme");
        return;
    };
    let file = xsettings_file();
    let Ok(mut held) = XSETTINGS.lock() else {
        return;
    };
    if held.0.is_empty() || std::fs::write(&file, &held.0).is_err() {
        return;
    }
    let result = fresh_signals(&mut std::process::Command::new(&program))
        .arg("-c")
        .arg(&file)
        .env("DISPLAY", display)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match result {
        Ok(mut child) => {
            held.1 = Some(child.id());
            std::thread::spawn(move || {
                let _ = child.wait();
                if let Ok(mut held) = XSETTINGS.lock() {
                    held.1 = None;
                }
            });
        }
        Err(error) => log!("drifter: cannot start xsettingsd: {error}"),
    }
}

pub fn tell_x11(text: String) {
    let pid = {
        let Ok(mut held) = XSETTINGS.lock() else {
            return;
        };
        if held.0 == text {
            return;
        }
        held.0 = text;
        if held.1.is_some()
            && let Err(error) = std::fs::write(xsettings_file(), &held.0)
        {
            log!("drifter: cannot write the X11 settings: {error}");
            return;
        }
        held.1
    };
    match (pid, X11.get()) {
        (Some(pid), _) => unsafe {
            libc::kill(pid as i32, libc::SIGHUP);
        },
        (None, Some(display)) => start_xsettings(display),
        (None, None) => {}
    }
}

pub fn launch(argv: &[String], socket: &str) {
    launch_in(argv, None, socket);
}

pub fn launch_in(argv: &[String], cwd: Option<&str>, socket: &str) {
    let Some((program, rest)) = argv.split_first() else {
        return;
    };
    let mut command = std::process::Command::new(program);
    if let Some(dir) = cwd.filter(|dir| std::path::Path::new(dir).is_dir()) {
        command.current_dir(dir);
    }
    fresh_signals(&mut command);
    if let Ok(held) = ENV.lock() {
        for (key, value) in held.iter() {
            command.env(key, value);
        }
    }
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
        Err(error) => log!("drifter: cannot start {program}: {error}"),
    }
}

pub fn open_folder(path: &str, chosen: Option<&drifter_core::Launch>, socket: &str) {
    open_as(path, drifter_core::desktop::Role::Files, chosen, socket);
}

pub fn open_as(path: &str, role: drifter_core::desktop::Role, chosen: Option<&drifter_core::Launch>, socket: &str) {
    if let Some(command) = chosen.and_then(|item| item.command()) {
        start(command, &[path], socket);
        return;
    }
    let app = chosen
        .and_then(|item| item.app().map(str::to_string))
        .or_else(|| drifter_core::desktop::system_default(role));
    if let Some(exec) = app.and_then(|id| find(&id)).and_then(|entry| entry.exec) {
        start(&exec, &[path], socket);
        return;
    }
    if PathBuf::from("/usr/bin/xdg-open").is_file() || PathBuf::from("/bin/xdg-open").is_file() {
        launch(&["xdg-open".to_string(), path.to_string()], socket);
        return;
    }
    log!("drifter: no app is installed to open {path}");
}

pub fn defaults(
    dock: &drifter_core::DockConfig,
    apps: &drifter_core::AppsConfig,
) -> Vec<drifter_core::Launch> {
    use drifter_core::desktop::Role;

    let mut chosen: Vec<drifter_core::Launch> = Vec::new();
    let wanted = [
        (dock.files, Role::Files),
        (dock.terminal, Role::Terminal),
        (dock.browser, Role::Browser),
    ];
    for (on, role) in wanted {
        if !on {
            continue;
        }
        let item = apps
            .get(role)
            .cloned()
            .or_else(|| drifter_core::desktop::system_default(role).map(drifter_core::Launch::App));
        if let Some(item) = item
            && !chosen.contains(&item)
        {
            chosen.push(item);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satellite_versions_read_from_any_describe_form() {
        assert_eq!(satellite_version("0.8.2\n"), Some((0, 8, 2)));
        assert_eq!(satellite_version("v0.8.3"), Some((0, 8, 3)));
        assert_eq!(satellite_version("v0.8.3-14-g7bf5e0d"), Some((0, 8, 3)));
        assert_eq!(satellite_version("0.8"), Some((0, 8, 0)));
        assert_eq!(satellite_version("abc"), None);
        assert!(satellite_version("0.8.2").unwrap() < SATELLITE_LEAST);
        assert!(satellite_version("v0.8.3-14-g7bf5e0d").unwrap() >= SATELLITE_LEAST);
    }

    #[test]
    fn x11_settings_name_the_theme_icons_and_pointer() {
        let text = xsettings_text("Adwaita-dark", Some("drifter-icons"), None, 24);
        assert_eq!(
            text,
            "Net/ThemeName \"Adwaita-dark\"\nNet/IconThemeName \"drifter-icons\"\nGtk/CursorThemeSize 24\n"
        );
    }
}
