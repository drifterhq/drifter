use std::path::Path;
use std::process::ExitCode;

use drifter_core::{Camera, Config, Tunables, Vec2, wallpaper};

#[cfg(target_os = "linux")]
mod render;
#[cfg(target_os = "linux")]
mod session;

#[cfg(target_os = "linux")]
fn load_image(path: &Path, what: &str) -> Option<drifter_core::Image> {
    match drifter_core::load_png(path) {
        Ok(image) => Some(image),
        Err(error) => {
            eprintln!("drifter: no {what}: {error}");
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn print_rect(what: &str, rect: Option<drifter_core::Rect>) {
    match rect {
        Some(rect) => println!(
            "            {what} at {} {} size {} x {}",
            rect.origin.x, rect.origin.y, rect.size.x, rect.size.y
        ),
        None => println!("            {what} not drawn"),
    }
}

fn present(path: &Path) -> &'static str {
    if path.is_file() { "found" } else { "missing" }
}

fn report(config: &Config) {
    let screen = Vec2::new(1920.0, 1080.0);
    let world = config.world_size(screen);
    let fit = Camera::fit_zoom(screen, world);
    let tunables = Tunables::default();
    let needed = wallpaper::required_size(config.workspace.wallpaper_mode, screen, world);

    println!(
        "workspace   {} x {} screens",
        config.workspace.width, config.workspace.height
    );
    println!("screen      {} x {}", screen.x, screen.y);
    println!("world       {} x {} px", world.x, world.y);
    println!("zoom range  {fit:.4} to {:.4}", Camera::WORK_ZOOM);
    println!("modifier    {:?}", config.input.modifier);
    println!("wallpaper   {:?}", config.workspace.wallpaper_mode);
    println!("            wants {} x {} px", needed.x, needed.y);
    match &config.workspace.wallpaper {
        Some(image) => println!("            {} ({})", image.display(), present(image)),
        None => println!("            none set"),
    }
    println!("drawers     {} px", tunables.drawer_height);
}

#[cfg(target_os = "linux")]
fn bootstrap(config: &Config) -> ExitCode {
    let mut seat = match session::open() {
        Ok(seat) => seat,
        Err(error) => {
            eprintln!("drifter: seat session failed: {error}");
            eprintln!("drifter: check that seatd is running");
            return ExitCode::FAILURE;
        }
    };

    println!("seat        {}", seat.name);
    println!("            active {}", seat.active);
    println!("            {} udev device(s)", seat.devices.len());
    for device in &seat.devices {
        println!("            {}", device.display());
    }

    let Some(card) = seat.devices.first().cloned() else {
        eprintln!("drifter: no drm device on seat {}", seat.name);
        return ExitCode::FAILURE;
    };

    let want = (1920, 1080);
    let night = [0.02, 0.05, 0.10, 1.0];

    let wallpaper = config
        .workspace
        .wallpaper
        .as_deref()
        .and_then(|path| load_image(path, "wallpaper"));

    let screen = match render::open(
        &mut seat.session,
        seat.notifier,
        &card,
        want,
        night,
        wallpaper.as_ref(),
    ) {
        Ok(screen) => screen,
        Err(error) => {
            eprintln!("drifter: {error}");
            return ExitCode::FAILURE;
        }
    };

    let found = screen.found();
    println!("screen      {}", found.connector);
    println!(
        "            {} x {} @ {} on {}",
        found.width,
        found.height,
        found.refresh,
        found.device.display()
    );
    println!("            gl {}", found.renderer);
    print_rect("wallpaper", found.wallpaper);
    println!("running     drifter holds the screen until it is killed");

    match screen.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("drifter: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn bootstrap(_config: &Config) -> ExitCode {
    println!("seat        not on linux, skipped");
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let path = Config::config_path();
    let config = match Config::load_from(&path) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("drifter: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("config      {} ({})", path.display(), present(&path));
    report(&config);
    bootstrap(&config)
}
