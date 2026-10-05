use crate::config::Style;

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
};

pub fn of(style: Option<Style>) -> Palette {
    match style {
        Some(Style::Light) => LIGHT,
        _ => DARK,
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
        assert_eq!(of(Some(Style::Light)), LIGHT);
        assert_eq!(of(Some(Style::Dark)), DARK);
        assert_eq!(of(None), DARK);
        assert!(light(LIGHT.back) > light(DARK.back));
    }
}
