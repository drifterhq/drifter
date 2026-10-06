use crate::config::{Night, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bar: [u8; 3],
    pub back: [u8; 3],
    pub idle: [u8; 3],
    pub surface: [u8; 3],
    pub well: [u8; 3],
    pub card: [u8; 3],
    pub raised: [u8; 3],
    pub hover: [u8; 3],
    pub rule: [u8; 3],
    pub off: [u8; 3],
    pub pick: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub alert: [u8; 3],
    pub glyph: [u8; 3],
    pub shade: [u8; 3],
    pub veil: u8,
    pub rim: [u8; 3],
    pub rim_alpha: u8,
    pub drop: u8,
}

pub const DARK: Palette = Palette {
    bar: [16, 18, 26],
    back: [30, 33, 42],
    idle: [24, 26, 33],
    surface: [37, 40, 50],
    well: [30, 32, 40],
    card: [42, 45, 57],
    raised: [52, 57, 72],
    hover: [62, 68, 86],
    rule: [58, 62, 78],
    off: [72, 77, 94],
    pick: [98, 105, 128],
    ink: [236, 238, 244],
    dim: [146, 152, 166],
    alert: [255, 120, 110],
    glyph: [226, 230, 238],
    shade: [255, 255, 255],
    veil: 165,
    rim: [255, 255, 255],
    rim_alpha: 34,
    drop: 100,
};

pub const LIGHT: Palette = Palette {
    bar: [238, 239, 243],
    back: [248, 248, 250],
    idle: [236, 237, 241],
    surface: [244, 245, 248],
    well: [234, 236, 240],
    card: [255, 255, 255],
    raised: [226, 229, 236],
    hover: [212, 217, 228],
    rule: [216, 219, 226],
    off: [196, 201, 212],
    pick: [178, 187, 206],
    ink: [26, 28, 34],
    dim: [100, 106, 120],
    alert: [208, 58, 48],
    glyph: [44, 48, 58],
    shade: [0, 0, 0],
    veil: 150,
    rim: [0, 0, 0],
    rim_alpha: 36,
    drop: 56,
};

pub fn resolve(style: Option<Style>, night: &Night, hour: u32) -> Style {
    match style {
        Some(Style::Light) => Style::Light,
        Some(Style::Auto) => match crate::night::is_night(night.start, night.end, hour) {
            true => Style::Dark,
            false => Style::Light,
        },
        _ => Style::Dark,
    }
}

pub fn now(style: Option<Style>, night: &Night) -> Style {
    resolve(style, night, crate::night::hour_now())
}

pub fn of(style: Style) -> Palette {
    match style {
        Style::Light => LIGHT,
        _ => DARK,
    }
}

pub const GTK_IMPORT: &str = "@import url(\"drifter.css\");";

pub fn with_import(text: &str) -> Option<String> {
    if text.lines().any(|line| line.trim() == GTK_IMPORT) {
        return None;
    }
    Some(format!("{GTK_IMPORT}\n{text}"))
}

const SWITCH: &str = "--headerbar-border-color";

fn between(light: [u8; 3], dark: [u8; 3]) -> String {
    let channel = |at: usize| {
        let step = (f64::from(dark[at]) - f64::from(light[at])) / 255.0;
        let sign = if step < 0.0 { '-' } else { '+' };
        format!("calc({} {sign} r * {:.6})", light[at], step.abs())
    };
    format!(
        "rgb(from var({SWITCH}) {} {} {} / 1)",
        channel(0),
        channel(1),
        channel(2)
    )
}

pub fn adwaita_colors() -> Vec<(&'static str, [u8; 3], [u8; 3])> {
    let (l, d) = (LIGHT, DARK);
    vec![
        ("window-bg-color", l.back, d.back),
        ("window-fg-color", l.ink, d.ink),
        ("view-bg-color", l.card, d.idle),
        ("view-fg-color", l.ink, d.ink),
        ("headerbar-bg-color", l.surface, d.surface),
        ("headerbar-fg-color", l.ink, d.ink),
        ("headerbar-backdrop-color", l.back, d.back),
        ("sidebar-bg-color", l.surface, d.surface),
        ("sidebar-fg-color", l.ink, d.ink),
        ("sidebar-backdrop-color", l.back, d.back),
        ("secondary-sidebar-bg-color", l.well, d.well),
        ("secondary-sidebar-fg-color", l.ink, d.ink),
        ("dialog-bg-color", l.surface, d.surface),
        ("dialog-fg-color", l.ink, d.ink),
        ("popover-bg-color", l.card, d.raised),
        ("popover-fg-color", l.ink, d.ink),
        ("card-fg-color", l.ink, d.ink),
        ("thumbnail-bg-color", l.card, d.card),
        ("thumbnail-fg-color", l.ink, d.ink),
    ]
}

pub fn gtk4_css() -> String {
    let lines: String = adwaita_colors()
        .into_iter()
        .map(|(name, light, dark)| format!("  --{name}: {};\n", between(light, dark)))
        .collect();
    format!(":root {{\n{lines}}}\n")
}

pub fn gtk4_dir() -> std::path::PathBuf {
    crate::config::xdg_dir("XDG_CONFIG_HOME", ".config").join("gtk-4.0")
}

pub fn install_gtk4_css(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let ours = dir.join("drifter.css");
    let css = gtk4_css();
    if std::fs::read_to_string(&ours).ok().as_deref() != Some(css.as_str()) {
        std::fs::write(&ours, css)?;
    }
    let main = dir.join("gtk.css");
    let text = match std::fs::read_to_string(&main) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    match with_import(&text) {
        Some(text) => std::fs::write(&main, text),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(color: [u8; 3]) -> f64 {
        color.iter().map(|c| f64::from(*c)).sum::<f64>() / 3.0
    }

    #[test]
    fn text_stands_out_from_every_background_in_both_sets() {
        for palette in [DARK, LIGHT] {
            for back in [palette.bar, palette.back, palette.surface, palette.card, palette.well, palette.raised] {
                assert!((light(palette.ink) - light(back)).abs() > 150.0, "{palette:?}");
                assert!((light(palette.dim) - light(back)).abs() > 60.0, "{palette:?}");
                assert!((light(palette.glyph) - light(back)).abs() > 150.0, "{palette:?}");
            }
        }
    }

    #[test]
    fn only_light_asks_for_the_light_set() {
        assert_eq!(of(Style::Light), LIGHT);
        assert_eq!(of(Style::Dark), DARK);
        assert!(light(LIGHT.back) > light(DARK.back));
    }

    #[test]
    fn auto_is_dark_through_the_night_hours_and_light_in_the_day() {
        let night = Night::default();
        let auto = Some(Style::Auto);
        assert_eq!(resolve(auto, &night, 21), Style::Dark);
        assert_eq!(resolve(auto, &night, 2), Style::Dark);
        assert_eq!(resolve(auto, &night, 7), Style::Light);
        assert_eq!(resolve(auto, &night, 12), Style::Light);
        assert_eq!(resolve(auto, &night, 20), Style::Light);
        let day = Night { start: 9, end: 17, ..Night::default() };
        assert_eq!(resolve(auto, &day, 10), Style::Dark);
        assert_eq!(resolve(auto, &day, 18), Style::Light);
    }

    #[test]
    fn a_fixed_style_ignores_the_hour_and_none_is_dark() {
        let night = Night::default();
        for hour in 0..24 {
            assert_eq!(resolve(Some(Style::Light), &night, hour), Style::Light);
            assert_eq!(resolve(Some(Style::Dark), &night, hour), Style::Dark);
            assert_eq!(resolve(None, &night, hour), Style::Dark);
        }
    }

    #[test]
    fn the_gtk_import_goes_on_top_once_and_keeps_the_users_own_css() {
        assert_eq!(with_import("").as_deref(), Some("@import url(\"drifter.css\");\n"));
        let mine = "label { color: red; }\n";
        let added = with_import(mine).unwrap();
        assert!(added.starts_with(GTK_IMPORT));
        assert!(added.ends_with(mine));
        assert_eq!(with_import(&added), None);
    }

    #[test]
    fn installing_the_gtk4_css_keeps_the_users_file_and_runs_twice_cleanly() {
        let dir = std::env::temp_dir().join(format!("drifter-gtk4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gtk.css"), "label { color: red; }\n").unwrap();
        install_gtk4_css(&dir).unwrap();
        install_gtk4_css(&dir).unwrap();
        let main = std::fs::read_to_string(dir.join("gtk.css")).unwrap();
        assert_eq!(main, format!("{GTK_IMPORT}\nlabel {{ color: red; }}\n"));
        assert_eq!(std::fs::read_to_string(dir.join("drifter.css")).unwrap(), gtk4_css());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_gtk4_css_lands_on_each_set_from_the_scheme_switch() {
        let solve = |text: &str, r: f64| -> f64 {
            let inner = text.trim_start_matches("calc(").trim_end_matches(')');
            let parts: Vec<&str> = inner.split_whitespace().collect();
            let base: f64 = parts[0].parse().unwrap();
            let step: f64 = parts[4].parse().unwrap();
            match parts[1] {
                "-" => base - r * step,
                _ => base + r * step,
            }
        };
        for (_, light, dark) in adwaita_colors() {
            let rule = between(light, dark);
            let calcs: Vec<String> = rule
                .split("calc(")
                .skip(1)
                .map(|part| format!("calc({}", &part[..part.find(')').unwrap() + 1]))
                .collect();
            assert_eq!(calcs.len(), 3, "{rule}");
            for at in 0..3 {
                assert_eq!(solve(&calcs[at], 0.0).round() as u8, light[at], "{rule}");
                assert_eq!(solve(&calcs[at], 255.0).round() as u8, dark[at], "{rule}");
            }
        }
        let css = gtk4_css();
        assert!(css.starts_with(":root {\n"));
        assert!(css.contains("--window-bg-color: rgb(from var(--headerbar-border-color) calc(248 - r * "));
        assert!(!css.contains("--headerbar-border-color:"));
        assert_eq!(css.matches('{').count(), css.matches('}').count());
    }
}
