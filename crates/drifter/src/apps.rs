use std::path::PathBuf;

pub struct Entry {
    pub name: Option<String>,
    pub icon: Option<PathBuf>,
    pub exec: Option<String>,
}

pub fn find(app_id: &str) -> Option<Entry> {
    let file = desktop_file(app_id)?;
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
            let _ = X11.set(display);
            std::thread::spawn(move || {
                let _ = child.wait();
                log!("drifter: xwayland-satellite stopped");
            });
        }
        Err(error) => log!("drifter: cannot start xwayland-satellite: {error}"),
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

pub fn file_manager() -> Option<String> {
    drifter_core::desktop::system_default(drifter_core::desktop::Role::Files)
}

pub fn open_folder(path: &str, chosen: Option<&drifter_core::Launch>, socket: &str) {
    if let Some(command) = chosen.and_then(|item| item.command()) {
        start(command, &[path], socket);
        return;
    }
    let app = chosen
        .and_then(|item| item.app().map(str::to_string))
        .or_else(file_manager);
    if let Some(exec) = app.and_then(|id| find(&id)).and_then(|entry| entry.exec) {
        start(&exec, &[path], socket);
        return;
    }
    if PathBuf::from("/usr/bin/xdg-open").is_file() || PathBuf::from("/bin/xdg-open").is_file() {
        launch(&["xdg-open".to_string(), path.to_string()], socket);
        return;
    }
    log!("drifter: no file manager is installed, so {path} cannot be opened");
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
