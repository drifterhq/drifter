use std::collections::HashMap;
use std::path::PathBuf;

use drifter_core::settings::{Art, Fonts, Look, Model, Section};
use drifter_core::sheet::{For, Sheet};
use drifter_core::stats;
use drifter_core::{Config, Font, Image, Launch, Vec2};

fn save(image: &Image, path: &std::path::Path) {
    let mut image = image.clone();
    image.straighten();
    let file = std::fs::File::create(path).expect("the file opens");
    let mut encoder = png::Encoder::new(file, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&image.rgba))
        .expect("the png writes");
    println!("{}", path.display());
}

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
    let mut model = Model::read(&config, &mounts, |mount| {
        Some(
            if mount.device == "254:1" {
                "vda1"
            } else {
                "sda3"
            }
            .to_string(),
        )
    });
    model.add(For::Login, Launch::App("firefox".into()));
    model.add(
        For::Login,
        Launch::Full {
            app: None,
            command: Some("foot -e htop".into()),
            icon: None,
        },
    );
    model.see_monitors(&[
        ("eDP-1".into(), Vec2::new(2880.0, 1800.0)),
        ("HDMI-A-1".into(), Vec2::new(1920.0, 1080.0)),
    ]);
    model.add(For::Dock, Launch::App("foot".into()));
    model.add(For::Dock, Launch::App("firefox".into()));
    if let Some(folder) = config
        .wallpaper_path()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
    {
        model.choose_folder(folder);
    }
    let thumbs: HashMap<PathBuf, Image> = model
        .pictures
        .iter()
        .filter_map(|path| {
            let image = drifter_core::image::load(path).ok()?;
            Some((path.clone(), drifter_core::settings::cover(&image, 128, 72)))
        })
        .collect();
    model.about = drifter_core::system::about(Some("GLES on a test GPU"), &Config::config_path());
    let icons = HashMap::new();
    let look = Look::default();
    let size = Vec2::new(860.0, 640.0);
    for section in Section::ALL {
        let art = Art {
            wallpaper: wallpaper.as_ref(),
            icons: &icons,
            thumbs: &thumbs,
            sheet: None,
        };
        let image = look.paint(size, section, &model, 0.0, &fonts, &art);
        let name = section.name().to_lowercase().replace([' ', '&'], "");
        save(&image, &out.join(format!("settings-{name}.png")));
    }
    let apps = drifter_core::desktop::installed();
    let picker = Sheet::picker(For::Dock, apps);
    let files = Sheet::files(
        For::Folder,
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default(),
    );
    for (name, sheet) in [("picker", &picker), ("files", &files)] {
        let art = Art {
            wallpaper: wallpaper.as_ref(),
            icons: &icons,
            thumbs: &thumbs,
            sheet: Some(sheet),
        };
        let image = look.paint(size, Section::Dock, &model, 0.0, &fonts, &art);
        save(&image, &out.join(format!("settings-sheet-{name}.png")));
    }
}
