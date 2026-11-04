use std::path::Path;
use std::process::ExitCode;

use drifter_core::{Camera, Config, Vec2, wallpaper};

macro_rules! out {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

#[cfg(target_os = "linux")]
mod a11y;
#[cfg(target_os = "linux")]
mod airdrop;
#[cfg(target_os = "linux")]
mod apps;
#[cfg(target_os = "linux")]
mod auth;
#[cfg(target_os = "linux")]
mod clipboard;
#[cfg(target_os = "linux")]
mod cursors;
#[cfg(target_os = "linux")]
mod disks;
#[cfg(target_os = "linux")]
mod greeter;
#[cfg(target_os = "linux")]
mod keyring;
#[cfg(target_os = "linux")]
mod logind;
#[cfg(target_os = "linux")]
mod launcher;
mod media;
#[cfg(target_os = "linux")]
mod filemanager;
#[cfg(target_os = "linux")]
mod notes;
#[cfg(target_os = "linux")]
mod paint;
#[cfg(target_os = "linux")]
mod polkit;
#[cfg(target_os = "linux")]
mod portal;
#[cfg(target_os = "linux")]
mod power;
#[cfg(target_os = "linux")]
mod protocols;
#[cfg(target_os = "linux")]
mod render;
#[cfg(target_os = "linux")]
mod screencopy;
#[cfg(target_os = "linux")]
mod session;
#[cfg(target_os = "linux")]
mod shell;
#[cfg(target_os = "linux")]
mod spatial;
#[cfg(target_os = "linux")]
mod stats;
#[cfg(target_os = "linux")]
mod tearing;
#[cfg(target_os = "linux")]
mod tray;
#[cfg(target_os = "linux")]
mod wlr_output;
#[cfg(target_os = "linux")]
mod wlr_toplevel;

#[cfg(target_os = "linux")]
#[cfg(target_os = "linux")]
fn print_rect(what: &str, rect: Option<drifter_core::Rect>) {
    match rect {
        Some(rect) => out!(
            "            {what} at {} {} size {} x {}",
            rect.origin.x, rect.origin.y, rect.size.x, rect.size.y
        ),
        None => out!("            {what} not drawn"),
    }
}

fn present(path: &Path) -> &'static str {
    if path.is_file() { "found" } else { "missing" }
}

fn report(config: &Config) {
    let screen = Vec2::new(1920.0, 1080.0);
    let world = config.world_size(screen);
    let fit = Camera::fit_zoom(screen, world);
    let needed = wallpaper::required_size(config.workspace.wallpaper_mode, screen, world);

    out!(
        "workspace   {} x {} screens",
        config.workspace.width, config.workspace.height
    );
    out!("screen      {} x {}", screen.x, screen.y);
    out!("world       {} x {} px", world.x, world.y);
    out!("zoom range  {fit:.4} to {:.4}", Camera::WORK_ZOOM);
    out!("modifier    {:?}", config.input.modifier);
    out!("wallpaper   {:?}", config.workspace.wallpaper_mode);
    out!("            wants {} x {} px", needed.x, needed.y);
    match &config.wallpaper_path() {
        Some(image) => out!("            {} ({})", image.display(), present(image)),
        None => out!("            none set"),
    }
    out!(
        "drawers     top {} px, bottom {} px",
        config.drawers.top, config.drawers.bottom
    );
}

#[cfg(target_os = "linux")]
fn bootstrap(config: &Config) -> ExitCode {
    let mut seat = match session::open() {
        Ok(seat) => seat,
        Err(error) => {
            log!("drifter: seat session failed: {error}");
            log!("drifter: check that seatd is running");
            return ExitCode::FAILURE;
        }
    };

    out!("seat        {}", seat.name);
    out!("            active {}", seat.active);
    out!("            {} udev device(s)", seat.devices.len());
    for device in &seat.devices {
        out!("            {}", device.display());
    }

    let Some(card) = seat.devices.first().cloned() else {
        log!("drifter: no drm device on seat {}", seat.name);
        return ExitCode::FAILURE;
    };

    let virtual_size = (1920, 1080);
    let night = [0.02, 0.05, 0.10, 1.0];

    let wallpaper = config.wallpaper_path().and_then(|path| {
        drifter_core::image::load_covering(&path, 5120.0, 2880.0)
            .map_err(|error| log!("drifter: no wallpaper: {error}"))
            .ok()
    });

    mark("seat open and wallpaper read");
    let screen = match render::open(
        Some(render::Seat {
            session: &mut seat.session,
            events: seat.notifier,
            card: &card,
        }),
        config,
        virtual_size,
        night,
        wallpaper,
    ) {
        Ok(screen) => screen,
        Err(error) => {
            log!("drifter: {error}");
            return ExitCode::FAILURE;
        }
    };

    let found = screen.found();
    out!("screen      {}", found.connector);
    out!(
        "            {} x {} @ {} on {}",
        found.width,
        found.height,
        found.refresh,
        found.device.display()
    );
    out!("            gl {}", found.renderer);
    print_rect("wallpaper", found.wallpaper);
    out!(
        "drawer      {} x {} px, y {} to {}",
        found.drawer.size.x,
        found.drawer.size.y,
        found.drawer.origin.y,
        found.drawer.max().y
    );
    out!(
        "            corners reach {} px further at the two ends",
        found.corner
    );
    out!("wayland     {}", found.socket);
    out!(
        "camera      at {} {} zoom {:.4}",
        found.camera.center.x, found.camera.center.y, found.camera.zoom
    );
    out!("running     drifter holds the screen until it is killed");

    match screen.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log!("drifter: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn bootstrap(_config: &Config) -> ExitCode {
    out!("seat        not on linux, skipped");
    ExitCode::SUCCESS
}

fn keep_crashes() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let folder = Config::state_dir();
        let when = drifter_core::tray::now("%Y-%m-%d %H:%M:%S").unwrap_or_default();
        let trace = std::backtrace::Backtrace::force_capture();
        let entry = format!("{when} drifter panicked: {info}\n{trace}\n");
        let written = std::fs::create_dir_all(&folder).and_then(|()| {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(folder.join("crash.log"))?
                .write_all(entry.as_bytes())
        });
        if let Err(error) = written {
            log!("drifter: cannot write the crash log: {error}");
        }
        default(info);
    }));
}

#[cfg(target_os = "linux")]
fn greeting() -> Option<std::path::PathBuf> {
    if !greeter::active() {
        return None;
    }
    let made = std::path::PathBuf::from("/etc/drifter/greeter.toml");
    let shared = Path::new(drifter_core::config::SHARED).join("default.toml");
    Some(if made.is_file() { made } else { shared })
}

#[cfg(not(target_os = "linux"))]
fn greeting() -> Option<std::path::PathBuf> {
    None
}

pub static CONFIG_ERROR: std::sync::OnceLock<String> = std::sync::OnceLock::new();

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub fn started() -> std::time::Instant {
    *STARTED.get_or_init(std::time::Instant::now)
}

pub fn mark(what: &str) {
    log!("timing      {what} at {} ms", started().elapsed().as_millis());
}

fn main() -> ExitCode {
    started();
    keep_crashes();
    let path = match greeting() {
        Some(path) => path,
        None => {
            let path = Config::config_path();
            match Config::first_run(&path) {
                Ok(true) => out!("config      made {} from the defaults", path.display()),
                Ok(false) => {}
                Err(error) => log!("drifter: cannot make {}: {error}", path.display()),
            }
            path
        }
    };
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(error) => {
            log!("drifter: {error}, so drifter uses its defaults");
            let _ = CONFIG_ERROR.set(error.to_string());
            Config::default()
        }
    };

    let language = drifter_core::lang::use_config(config.appearance.language.as_deref());
    out!("config      {} ({}), language {language}", path.display(), present(&path));
    report(&config);
    bootstrap(&config)
}
