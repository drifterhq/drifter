use crate::config::Accent;
use crate::image::Image;

pub const CYCLE: f64 = 8.0;
pub const FALLBACK: [u8; 3] = [53, 132, 228];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paper {
    Live,
    Still([u8; 3]),
    Plain,
}

impl Paper {
    pub fn live(self) -> bool {
        self == Paper::Live
    }
}

pub fn hint(wallpaper: Option<&std::path::Path>) -> Paper {
    match wallpaper {
        Some(path) if crate::shader::is_shader(path) => Paper::Live,
        _ => Paper::Plain,
    }
}

pub fn hour_now() -> f64 {
    (crate::shader::hour_now() * 12.0).floor() / 12.0
}

pub fn of_hour(hour: f64) -> [u8; 3] {
    let a = crate::shader::sky(hour).a;
    settle([a[0], a[1], a[2]])
}

pub fn cycling(seconds: f64) -> [u8; 3] {
    of_hour(seconds / CYCLE * 24.0)
}

pub fn of_picture(image: &Image) -> [u8; 3] {
    let count = (image.width as usize) * (image.height as usize);
    if count == 0 || image.rgba.len() < count * 4 {
        return FALLBACK;
    }
    let step = (count / 4096).max(1);
    let mut sum = [0.0; 3];
    let mut weight = 0.0;
    for at in (0..count).step_by(step) {
        let px = &image.rgba[at * 4..at * 4 + 4];
        if px[3] < 128 {
            continue;
        }
        let rgb = [px[0], px[1], px[2]].map(|v| f64::from(v) / 255.0);
        let (_, s, l) = hsl(rgb);
        let w = s * (1.0 - (2.0 * l - 1.0).abs()) + 0.02;
        for c in 0..3 {
            sum[c] += rgb[c] * w;
        }
        weight += w;
    }
    if weight <= 0.0 {
        return FALLBACK;
    }
    settle(sum.map(|v| v / weight))
}

pub fn resolve(accent: Option<Accent>, paper: Paper, hour: f64) -> Option<[u8; 3]> {
    match accent? {
        Accent::Wallpaper => Some(match paper {
            Paper::Live => of_hour(hour),
            Paper::Still(color) => color,
            Paper::Plain => FALLBACK,
        }),
        fixed => Some(fixed.color()),
    }
}

pub fn nearest(color: [u8; 3]) -> Accent {
    let (h, s, _) = hsl(color.map(|v| f64::from(v) / 255.0));
    Accent::ALL
        .into_iter()
        .filter(|accent| *accent != Accent::Wallpaper)
        .min_by(|a, b| {
            let gap = |accent: &Accent| {
                let (ah, as_, _) = hsl(accent.color().map(|v| f64::from(v) / 255.0));
                let turn = (h - ah).abs().min(1.0 - (h - ah).abs());
                turn * s.min(as_) * 4.0 + (s - as_).abs()
            };
            gap(a).total_cmp(&gap(b))
        })
        .unwrap_or(Accent::Blue)
}

fn settle(rgb: [f64; 3]) -> [u8; 3] {
    let (h, s, l) = hsl(rgb);
    let s = s.clamp(0.5, 0.82);
    let l = l.clamp(0.42, 0.54);
    from_hsl(h, s, l).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn hsl(rgb: [f64; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb;
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    let l = (high + low) / 2.0;
    let d = high - low;
    if d <= 1e-9 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-9);
    let h = if high == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if high == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s.min(1.0), l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h.rem_euclid(1.0) * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r + m, g + m, b + m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lightness(color: [u8; 3]) -> f64 {
        hsl(color.map(|v| f64::from(v) / 255.0)).2
    }

    #[test]
    fn the_hour_accent_stays_readable_all_day() {
        let mut hour = 0.0;
        while hour < 24.0 {
            let color = of_hour(hour);
            let l = lightness(color);
            assert!((0.41..=0.55).contains(&l), "hour {hour} gives {color:?} at lightness {l}");
            hour += 0.25;
        }
    }

    #[test]
    fn the_hour_accent_follows_the_sky() {
        let night = of_hour(1.0);
        let dusk = of_hour(19.0);
        assert!(night[1] > night[0], "night is green, got {night:?}");
        assert!(dusk[0] > dusk[2], "dusk is warm, got {dusk:?}");
    }

    #[test]
    fn the_cycle_runs_through_the_day_and_wraps() {
        assert_eq!(cycling(0.0), of_hour(0.0));
        assert_eq!(cycling(CYCLE / 2.0), of_hour(12.0));
        assert_eq!(cycling(CYCLE * 3.25), of_hour(6.0));
    }

    #[test]
    fn a_picture_gives_its_strongest_color() {
        let mut rgba = Vec::new();
        for at in 0..100 {
            let px: [u8; 4] = if at % 5 == 0 { [220, 40, 40, 255] } else { [100, 120, 160, 255] };
            rgba.extend_from_slice(&px);
        }
        let color = of_picture(&Image { width: 10, height: 10, rgba });
        assert!(color[0] > color[1] + 40 && color[0] > color[2] + 40, "got {color:?}");
    }

    #[test]
    fn wallpaper_resolves_by_the_paper_and_fixed_colors_stay() {
        assert_eq!(resolve(Some(Accent::Wallpaper), Paper::Live, 9.0), Some(of_hour(9.0)));
        assert_eq!(resolve(Some(Accent::Wallpaper), Paper::Still([1, 2, 3]), 9.0), Some([1, 2, 3]));
        assert_eq!(resolve(Some(Accent::Wallpaper), Paper::Plain, 9.0), Some(FALLBACK));
        assert_eq!(resolve(Some(Accent::Red), Paper::Live, 9.0), Some(Accent::Red.color()));
        assert_eq!(resolve(None, Paper::Live, 9.0), None);
    }

    #[test]
    fn only_a_shader_wallpaper_is_live() {
        assert_eq!(hint(Some(std::path::Path::new("/w/aurora.shader"))), Paper::Live);
        assert_eq!(hint(Some(std::path::Path::new("/w/lake.jpg"))), Paper::Plain);
        assert_eq!(hint(None), Paper::Plain);
    }

    #[test]
    fn nearest_names_a_gnome_accent() {
        assert_eq!(nearest(Accent::Orange.color()), Accent::Orange);
        assert_eq!(nearest(Accent::Teal.color()), Accent::Teal);
        assert_eq!(nearest([120, 128, 136]), Accent::Slate);
        assert_ne!(nearest(of_hour(1.0)), Accent::Wallpaper);
    }
}
