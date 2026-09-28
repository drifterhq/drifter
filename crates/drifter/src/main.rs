use std::path::Path;
use std::process::ExitCode;

use drifter_core::{Camera, Config, Rect, Tunables, Vec2, wallpaper};

#[cfg(target_os = "linux")]
mod render;
#[cfg(target_os = "linux")]
mod session;

fn present(path: &Path) -> &'static str {
    if path.is_file() { "found" } else { "missing" }
}

fn report(config: &Config) {
    let screen = Vec2::new(1920.0, 1080.0);
    let world = config.world_size(screen);
    let fit = Camera::fit_zoom(screen, world);
    let tunables = Tunables::default();
    let needed = wallpaper::required_size(config.workspace.wallpaper_mode, screen, world);
    let splash = Config::splash_path();

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
    println!("splash      {} ({})", splash.display(), present(&splash));
    match drifter_core::load_png(&splash) {
        Ok(image) => {
            let rect = Rect::fit_centered(image.size(), screen);
            println!("            {} x {} decoded", image.width, image.height);
            println!(
                "            draws at {} {} size {} x {}",
                rect.origin.x, rect.origin.y, rect.size.x, rect.size.y
            );
        }
        Err(error) => println!("            not usable: {error}"),
    }
    println!("drawers     {} px", tunables.drawer_height);
}

#[cfg(target_os = "linux")]
fn bootstrap(config: &Config) -> ExitCode {
    use std::time::Duration;

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
    let orange = [1.0, 0.4, 0.0, 1.0];
    match render::show_color(
        &mut seat.session,
        &card,
        want,
        orange,
        Duration::from_secs(5),
    ) {
        Ok(shown) => {
            println!("screen      {}", shown.connector);
            println!(
                "            {} x {} @ {} on {}",
                shown.width,
                shown.height,
                shown.refresh,
                shown.device.display()
            );
            println!("            gl {}", shown.renderer);
            println!("            {} frames in {:?}", shown.frames, shown.held);
        }
        Err(error) => {
            eprintln!("drifter: {error}");
            return ExitCode::FAILURE;
        }
    }

    let _ = config;
    match session::watch(seat.notifier, Duration::from_millis(200)) {
        Ok((paused, activated)) => {
            println!("            {paused} paused, {activated} activated while settling");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("drifter: seat watch failed: {error}");
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
