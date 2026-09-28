use std::path::Path;
use std::process::ExitCode;

use drifter_core::{Camera, Config, Rect, Tunables, Vec2, wallpaper};

fn present(path: &Path) -> &'static str {
    if path.is_file() { "found" } else { "missing" }
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

    let screen = Vec2::new(1920.0, 1080.0);
    let world = config.world_size(screen);
    let fit = Camera::fit_zoom(screen, world);
    let tunables = Tunables::default();
    let needed = wallpaper::required_size(config.workspace.wallpaper_mode, screen, world);
    let splash = Config::splash_path();
    let splash_rect = Rect::fit_centered(screen, screen);

    println!("config      {} ({})", path.display(), present(&path));
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

    ExitCode::SUCCESS
}
