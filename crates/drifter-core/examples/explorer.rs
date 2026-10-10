use drifter_core::explorer::look::{Art, Fonts, Look, Paint};
use drifter_core::explorer::model::{Act, Model, Sheet};
use drifter_core::explorer::prefs::Mode;
use drifter_core::{Font, Vec2};
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap());
    let home = out.join("home");
    let _ = std::fs::remove_dir_all(&home);
    for dir in ["Documents/Work", "Documents/Taxes", "Downloads", "Pictures/2026", "Music", "Videos", "Desktop", "Projects/drifter"] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
    for (name, bytes) in [("notes.md", 2400usize), ("budget 2026.xlsx", 48000), ("holiday.jpg", 3_400_000), ("song.flac", 32_000_000), ("clip.mkv", 900_000_000), ("report.pdf", 220_000), ("archive.tar.gz", 5_000_000), ("main.rs", 9000), ("a very long file name that will not fit anywhere at all.txt", 10)] {
        std::fs::write(home.join("Documents").join(name), "fn main() {\n    println!(\"hello\");\n}\n".repeat(bytes.min(2000) / 40 + 1)).unwrap();
    }
    unsafe { std::env::set_var("HOME", &home) };
    let body = Font::find(None, 13.0).unwrap();
    let small = Font::find(None, 11.5).unwrap();
    let title = Font::find(None, 17.0).unwrap();
    let fonts = Fonts { body: &body, small: &small, title: &title };
    let art = Art::default();
    let size = Vec2::new(1100.0, 700.0);
    for dark in [false, true] {
        let palette = if dark { drifter_core::theme::DARK } else { drifter_core::theme::LIGHT };
        let look = Look::themed(&palette, [64, 128, 238]);
        let tag = if dark { "dark" } else { "light" };
        let shot = |model: &mut Model, name: &str| {
            let lines = model.lines();
            let image = look.paint(size, &Paint { model, lines: &lines, fonts: &fonts, art: &art });
            drifter_core::image::save_png(&image, &out.join(format!("{tag}-{name}.png"))).unwrap();
        };
        let mut model = Model::new(out.join("explorer.toml"), Some(home.join("Documents")));
        model.home = home.clone();
        model.prefs.mode = Mode::List;
        model.pick_only(home.join("Documents/holiday.jpg"));
        model.picked.insert(home.join("Documents/notes.md"));
        shot(&mut model, "list");
        model.act(Act::Show(Mode::Icons));
        shot(&mut model, "icons");
        model.act(Act::Show(Mode::Columns));
        model.pick_only(home.join("Documents/main.rs"));
        shot(&mut model, "columns");
        model.act(Act::Show(Mode::Gallery));
        shot(&mut model, "gallery");
        model.act(Act::Show(Mode::List));
        model.menu = None;
        let items = model.item_menu();
        model.menu = Some(drifter_core::explorer::model::Menu { items, at: Vec2::new(420.0, 200.0), hover: Some(2), open: None, kid_hover: None, bar: None });
        shot(&mut model, "menu");
        model.menu = None;
        let items = model.bar_menu(2);
        model.menu = Some(drifter_core::explorer::model::Menu { items, at: Vec2::new(90.0, 33.0), hover: Some(6), open: Some(6), kid_hover: Some(0), bar: Some(2) });
        shot(&mut model, "barmenu");
        model.menu = None;
        for tab in 0..4 {
            model.sheet = Some(Sheet::Prefs(tab));
            shot(&mut model, &format!("prefs{tab}"));
        }
        model.act(Act::Info);
        shot(&mut model, "info");
        model.sheet = None;
        model.act(Act::DeleteForever);
        shot(&mut model, "confirm");
        model.sheet = Some(Sheet::Keys);
        shot(&mut model, "keys");
        model.sheet = None;
        model.say("Cannot open Taxes: Permission denied (os error 13)", true);
        model.say("clip.mkv: No space left on device (os error 28)", true);
        model.hover = Some(drifter_core::explorer::look::Hit::Dismiss);
        shot(&mut model, "alert");
        model.hover = None;
        model.alerts.clear();
        model.start_job(drifter_core::explorer::ops::Job::Copy { from: vec![home.join("Documents/clip.mkv")], into: home.join("Videos") });
        model.start_job(drifter_core::explorer::ops::Job::EmptyTrash);
        for job in &mut model.jobs {
            job.started -= std::time::Duration::from_secs(20);
        }
        let first = &model.jobs[0].progress;
        first.total.store(4_700_000_000, std::sync::atomic::Ordering::Relaxed);
        first.bytes.store(1_300_000_000, std::sync::atomic::Ordering::Relaxed);
        shot(&mut model, "jobs");
        model.jobs.clear();
        model.effects.clear();
        model.act(Act::EditPath);
        shot(&mut model, "path");
    }
}
