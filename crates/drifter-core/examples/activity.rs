use std::collections::HashMap;
use std::path::PathBuf;

use drifter_core::activity::hour::{self, Reading, Share};
use drifter_core::activity::look::{Disk, Fonts, Look, Measure, Model, Power, Toast, View};
use drifter_core::activity::{Group, Kind, Member, Memory};
use drifter_core::{Config, Font, Image, Vec2};

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

fn group(key: u32, name: &str, detail: &str, kind: Kind, cpu: f64, mb: u64, members: &[&str]) -> Group {
    let people: Vec<Member> = members
        .iter()
        .enumerate()
        .map(|(at, name)| Member {
            pid: key + at as u32,
            name: name.to_string(),
            cpu: if at == 0 { cpu * 0.4 } else { cpu * 0.6 / (members.len() - 1).max(1) as f64 },
            mem_kb: mb * 1024 / members.len() as u64,
            ppid: key,
            start: 0,
        })
        .collect();
    Group {
        key: key.to_string(),
        name: name.to_string(),
        detail: detail.to_string(),
        icon: None,
        kind,
        root: key,
        argv: vec![format!("/usr/bin/{}", members.first().copied().unwrap_or(name))],
        members: people,
        cpu,
        mem_kb: mb * 1024,
        paused: false,
        protected: false,
        flatpak: false,
    }
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let config = Config::load_from(&Config::config_path()).unwrap_or_default();
    let path = config.font_path();
    let fonts = (
        Font::find(path.clone(), 13.0).expect("a font"),
        Font::find(path.clone(), 11.5).expect("a font"),
        Font::find(path, 22.0).expect("a font"),
    );
    let fonts = Fonts { body: &fonts.0, small: &fonts.1, title: &fonts.2 };
    let mut blender = group(3120, "Blender", "1 process", Kind::App, 1.2, 3480, &["blender"]);
    blender.paused = true;
    blender.detail = "Paused".into();
    let groups = vec![
        group(1880, "Foot", "Running cargo", Kind::App, 71.0, 1640, &["foot", "cargo", "rustc", "rustc", "rustc", "ld.lld"]),
        group(2211, "Firefox ESR", "6 processes", Kind::App, 9.4, 2180, &["firefox-esr", "Web Content", "Web Content", "Web Content", "GPU Process", "Socket Process"]),
        blender,
        group(2710, "Code", "3 processes", Kind::App, 3.9, 1210, &["code", "rust-analyzer", "code"]),
        group(2902, "Spotify", "2 processes", Kind::App, 2.4, 390, &["spotify", "spotify"]),
        group(2510, "Files", "1 process", Kind::App, 0.3, 150, &["nautilus"]),
        { let mut g = group(4000, "Calculator", "Flatpak · 1 process", Kind::App, 0.4, 90, &["gnome-calculator"]); g.flatpak = true; g },
        group(912, "Drifter", "Draws your screen", Kind::Drifter, 3.5, 240, &["drifter"]),
        group(1350, "File search index", "tracker-miner-fs-3", Kind::Background, 11.0, 310, &["tracker-miner-fs-3"]),
        group(980, "Sound", "pipewire · 3 processes", Kind::Background, 1.1, 44, &["pipewire", "wireplumber", "pipewire-pulse"]),
        group(640, "Network", "NetworkManager", Kind::System, 0.2, 22, &["NetworkManager"]),
        group(402, "System log", "systemd-journald", Kind::System, 0.3, 58, &["systemd-journald"]),
    ];
    let mut model = Model {
        groups,
        cores: 8,
        memory: Memory {
            total_kb: 16 * 1024 * 1024,
            available_kb: 5 * 1024 * 1024,
            swap_total_kb: 2 * 1024 * 1024,
            swap_free_kb: 2 * 1024 * 1024,
        },
        pressure: Some(0.0),
        power: Some(Power { watts: Some(14.2), left_hours: Some(3.17), charging: false, percent: 82 }),
        disk: Some(Disk { label: "NVME0N1P2".into(), used: 61, size_gb: 512.0 }),
        selected: Some("1880".into()),
        now: 1_790_000_000,
        ..Model::default()
    };
    let base = model.groups.clone();
    for tick in 0..60 {
        let t = tick as f64;
        for group in base.clone() {
            let wave = match group.name.as_str() {
                "Foot" if tick % 38 < 15 => 60.0 + 25.0 * (t / 2.3).sin().abs(),
                "Foot" => 0.6,
                _ => group.cpu * (0.7 + 0.3 * (t / 3.0 + group.root as f64).sin().abs()),
            };
            if let Some(held) = model.groups.iter_mut().find(|held| held.key == group.key) {
                held.cpu = if held.paused { 0.0 } else { wave };
            }
        }
        model.cpu = model.groups.iter().map(|group| group.cpu).sum::<f64>().min(100.0);
        model.remember();
    }
    model.arrange();
    let look = Look::themed(&drifter_core::theme::DARK, [64, 128, 238]);
    let size = Vec2::new(1180.0, 700.0);
    let icons = HashMap::new();
    save(&look.paint(size, &model, &fonts, &icons), &out.join("activity-apps.png"));

    model.open.insert("2211".into());
    model.selected = Some("2211".into());
    model.hold = 0.55;
    model.toast = Some(Toast { words: "Quit Spotify.".into(), reopen: true });
    save(&look.paint(size, &model, &fonts, &icons), &out.join("activity-open.png"));
    model.hold = 0.0;
    model.toast = None;

    let light = Look::themed(&drifter_core::theme::LIGHT, [64, 128, 238]);
    model.view = View::Background;
    model.selected = Some("640".into());
    model.arrange();
    save(&light.paint(size, &model, &fonts, &icons), &out.join("activity-background-light.png"));

    let mut text = String::new();
    for at in 0..hour::KEPT as u64 {
        let i = at as f64;
        let bump = |a: f64, b: f64, h: f64| if i >= a && i <= b { h * ((i - a) / (b - a) * std::f64::consts::PI).sin().sqrt() } else { 0.0 };
        let cargo = bump(28.0, 36.0, 82.0) + bump(86.0, 96.0, 88.0) + bump(112.0, 118.0, 76.0);
        let firefox = 6.0 + 3.0 * (i / 4.0).sin() + bump(58.0, 74.0, 18.0);
        let blender = if at < 8 { 0.0 } else { bump(38.0, 54.0, 58.0) + 1.5 };
        let code = 4.0 + 2.0 * (i / 6.0 + 1.0).sin();
        let tracker = bump(98.0, 140.0, 12.0);
        let mut top = vec![
            Share { name: "Foot".into(), icon: None, app: true, cpu: cargo, mem_mb: 900 + (cargo * 9.0) as u64 },
            Share { name: "Firefox ESR".into(), icon: None, app: true, cpu: firefox, mem_mb: 2100 },
            Share { name: "Code".into(), icon: None, app: true, cpu: code, mem_mb: 1200 },
            Share { name: "File search index".into(), icon: None, app: false, cpu: tracker, mem_mb: 300 },
        ];
        if at >= 8 {
            top.push(Share { name: "Blender".into(), icon: None, app: true, cpu: blender, mem_mb: 3400 });
        }
        let cpu = top.iter().map(|share| share.cpu).sum::<f64>() + 5.0;
        let mem = if (78..84).contains(&at) { 91.0 } else { 62.0 };
        let reading = Reading { at: model.now - (hour::KEPT as u64 - 1 - at) * hour::EVERY_SECONDS, cpu, mem, swap_mb: if at >= 80 { 640 } else { 0 }, top };
        text = hour::append(&text, &reading);
    }
    model.hour = hour::read(&text, model.now);
    model.moments = hour::moments(&model.hour);
    model.view = View::Hour;
    model.cursor = Some(model.hour[91].at);
    save(&look.paint(size, &model, &fonts, &icons), &out.join("activity-hour.png"));
    model.measure = Measure::Memory;
    model.cursor = Some(model.hour[80].at);
    save(&light.paint(size, &model, &fonts, &icons), &out.join("activity-hour-memory-light.png"));
    model.hour.clear();
    model.measure = Measure::Cpu;
    save(&look.paint(size, &model, &fonts, &icons), &out.join("activity-hour-empty.png"));

    use drifter_core::activity::glance;
    let card = drifter_core::glance::Card { width: glance::WIDTH, ..drifter_core::glance::Card::themed(&drifter_core::theme::DARK) };
    let card = drifter_core::glance::Card { back_alpha: 1.0, ..card };
    let fields = vec![("CPU".to_string(), 41u8), ("RAM".to_string(), 69u8), ("VDA1".to_string(), 61u8)];
    let history: Vec<u8> = model.machine.iter().map(|v| *v as u8).collect();
    let tops = glance::pick(&model.groups, model.memory.total_kb);
    let bar = Font::find(config.font_path(), 13.0).expect("a font");
    let small = Font::find(config.font_path(), 12.0).expect("a font");
    let layout = glance::Layout { card, rows: fields.len(), spark: true };
    for (name, open) in [("glance.png", None), ("glance-open.png", Some(1))] {
        let paint = glance::Paint {
            font: &small,
            bold: &bar,
            fields: &fields,
            history: &history[history.len() - 30..],
            tops: &tops,
            open,
            hover: None,
            icons: &icons,
            counts: (7, 4),
            accent: [10, 122, 255],
            undo: open.map(|_| "Spotify"),
            ready: true,
        };
        save(&glance::paint(&layout, &paint), &out.join(name));
    }
}
