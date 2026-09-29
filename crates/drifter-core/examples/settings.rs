//! Draws every settings section to a PNG, so the design can be checked
//! without a Wayland session: cargo run -p drifter-core --example settings -- <folder>

use std::path::PathBuf;

use drifter_core::settings::{Fonts, Look, Model, Section};
use drifter_core::stats;
use drifter_core::{Config, Font, Vec2};

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let config = Config::load_from(&Config::config_path()).expect("the config reads");
    let font = config.font_path();
    let fonts = (
        Font::find(font.clone(), 13.0).expect("a font"),
        Font::find(font.clone(), 11.5).expect("a font"),
        Font::find(font, 22.0).expect("a font"),
    );
    let fonts = Fonts {
        body: &fonts.0,
        small: &fonts.1,
        title: &fonts.2,
    };
    let wallpaper = config
        .wallpaper_path()
        .and_then(|path| drifter_core::image::load(&path).ok());
    let mounts = stats::mounts(
        "29 1 254:1 / / rw - ext4 /dev/vda1 rw\n31 29 8:3 / /home rw - ext4 /dev/sda3 rw\n",
    );
    let model = Model::read(&config, &mounts, |mount| {
        Some(
            if mount.device == "254:1" {
                "vda1"
            } else {
                "sda3"
            }
            .to_string(),
        )
    });
    let look = Look::default();
    let size = Vec2::new(860.0, 620.0);
    for section in Section::ALL {
        let mut image = look.paint(size, section, &model, 0.0, &fonts, wallpaper.as_ref());
        image.straighten();
        let name = section.name().to_lowercase().replace([' ', '&'], "");
        let path = out.join(format!("settings-{name}.png"));
        let file = std::fs::File::create(&path).expect("the file opens");
        let mut encoder = png::Encoder::new(file, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&image.rgba))
            .expect("the png writes");
        println!("{}", path.display());
    }
}
