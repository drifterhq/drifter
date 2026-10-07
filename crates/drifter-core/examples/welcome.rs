use std::path::{Path, PathBuf};

use drifter_core::desktop::Role;
use drifter_core::welcome::{Card, Choice, Device, Fonts, Lesson, Look, Paper, Tile, View, SIZE};
use drifter_core::{Config, Image, Vec2};

fn save(image: &Image, path: &Path) {
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

fn over(ground: &Image, piece: &Image, at: (u32, u32)) -> Image {
    let mut out = ground.clone();
    for y in 0..piece.height {
        for x in 0..piece.width {
            let from = ((y * piece.width + x) * 4) as usize;
            let to = (((y + at.1) * out.width + x + at.0) * 4) as usize;
            let a = f64::from(piece.rgba[from + 3]) / 255.0;
            for c in 0..3 {
                let v = f64::from(piece.rgba[from + c]) + f64::from(out.rgba[to + c]) * (1.0 - a);
                out.rgba[to + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let density: u32 = std::env::args().nth(2).and_then(|d| d.parse().ok()).unwrap_or(1);
    let hour: f64 = std::env::args().nth(3).and_then(|h| h.parse().ok()).unwrap_or(21.0);
    let light = std::env::args().nth(4).is_some_and(|word| word == "light");
    drifter_core::paint::set_density(density);
    let config = Config::load_from(&Config::config_path()).expect("the config reads");
    let fonts = Fonts::load(config.font_path(), false).expect("fonts");
    let folders = config.shader_folders();
    let papers: Vec<PathBuf> = drifter_core::shader::NAMES
        .iter()
        .filter_map(|name| drifter_core::shader::find(&format!("{name}.shader"), &folders))
        .collect();
    let screen = Vec2::new(1440.0, 900.0);
    let (sw, sh) = (screen.x as u32 * density, screen.y as u32 * density);
    let ground = drifter_core::shader::still(&papers[0], sw, sh, 40.0, hour);
    let frost = drifter_core::frost::Frost::build(&ground);
    let palette = match light {
        true => drifter_core::theme::LIGHT,
        false => drifter_core::theme::DARK,
    };
    let look = Look::themed(&palette, [53, 132, 228]);
    let rect = look.rect(screen);
    let mut view = View::new(Card::Hello);
    view.greeting = drifter_core::welcome::greeting(hour as u32, "Dana");
    view.papers = papers
        .iter()
        .enumerate()
        .map(|(at, path)| Paper {
            name: {
                let stem = drifter_core::shader::stem(path);
                let mut chars = stem.chars();
                chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
            },
            still: Some(drifter_core::welcome::thumb(path, hour, 30.0)),
            chosen: at == 0,
        })
        .collect();
    view.scales = drifter_core::welcome::scales_for(Vec2::new(2400.0, 1600.0));
    view.scale = 1.0;
    view.suggested = drifter_core::welcome::suggest(2400.0, 259.0, &view.scales);
    view.screen = "Built-in screen, 2400 by 1600".into();
    view.tiles = vec![
        Tile { role: Role::Browser, name: Some("Firefox ESR".into()), icon: None, busy: false, weak: false },
        Tile { role: Role::Mail, name: Some("Thunderbird".into()), icon: None, busy: false, weak: false },
        Tile { role: Role::Files, name: Some("Thunar File Manager".into()), icon: None, busy: false, weak: false },
        Tile { role: Role::Terminal, name: Some("Foot".into()), icon: None, busy: false, weak: false },
        Tile { role: Role::Editor, name: None, icon: None, busy: true, weak: false },
        Tile { role: Role::Images, name: Some("Firefox ESR".into()), icon: None, busy: false, weak: true },
        Tile { role: Role::Video, name: Some("mpv".into()), icon: None, busy: false, weak: false },
        Tile { role: Role::Music, name: None, icon: None, busy: false, weak: false },
    ];
    let shots: Vec<(String, View)> = {
        let mut shots = Vec::new();
        for card in Card::ALL {
            let mut each = view.clone();
            each.card = card;
            shots.push((format!("{}-{card:?}", card.index() + 1), each));
        }
        let mut keys = view.clone();
        keys.card = Card::Keys;
        keys.device = Device::Mouse;
        keys.focus = Lesson::Glide;
        keys.done = [true, false, false, false];
        keys.held = true;
        shots.push(("5b-Keys-mouse".into(), keys));
        let mut zoom = view.clone();
        zoom.card = Card::Keys;
        zoom.focus = Lesson::Overview;
        zoom.done = [true, true, false, false];
        shots.push(("5c-Keys-tap".into(), zoom));
        let mut choose = view.clone();
        choose.card = Card::Apps;
        choose.choosing = Some((
            Role::Editor,
            vec![
                Choice { name: "Mousepad".into(), icon: None, chosen: true, installed: true, busy: false },
                Choice { name: "Text Editor".into(), icon: None, chosen: false, installed: false, busy: true },
            ],
        ));
        shots.push(("6b-Apps-choose".into(), choose));
        shots
    };
    for (name, view) in shots {
        let mut card = look.paint(&view, &fonts, f64::from(palette.veil) / 255.0);
        let laid = (rect.origin, screen);
        drifter_core::frost::underlay(&mut card, f64::from(palette.veil) / 255.0, |point| {
            let p = laid.0 + point / f64::from(density);
            frost.at(p.x / laid.1.x, p.y / laid.1.y)
        });
        let mut shadow = drifter_core::paint::Canvas::new(sw / density, sh / density);
        shadow.shadow(rect, look.radius(), 40.0, 0.45);
        let lit = over(&ground, &shadow.image, (0, 0));
        let at = ((rect.origin.x as u32) * density, (rect.origin.y as u32) * density);
        let whole = over(&lit, &card, at);
        let _ = SIZE;
        save(&whole, &out.join(format!("welcome-{name}.png")));
    }
}
