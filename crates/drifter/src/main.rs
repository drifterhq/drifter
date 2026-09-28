use std::path::Path;
use std::process::ExitCode;

use drifter_core::{Camera, Config, Rect, Tunables, Vec2, wallpaper};

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
    let splash_rect = Rect::fit_centered(screen, screen);

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
    println!(
        "            draws at {} {} size {} x {}",
        splash_rect.origin.x, splash_rect.origin.y, splash_rect.size.x, splash_rect.size.y
    );
    println!("drawers     {} px", tunables.drawer_height);
}

#[cfg(target_os = "linux")]
fn bootstrap() -> ExitCode {
    match session::bootstrap(std::time::Duration::from_secs(2)) {
        Ok(found) => {
            println!("seat        {}", found.seat);
            println!("            active {}", found.active);
            println!("            {} udev device(s)", found.devices);
            println!("            {} seat event(s) in 2s", found.events);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("drifter: seat session failed: {error}");
            eprintln!("drifter: check that seatd is running and that you are on a tty");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn bootstrap() -> ExitCode {
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
    bootstrap()
}
