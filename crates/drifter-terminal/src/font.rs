use std::collections::HashMap;

use drifter_core::{Font, Image};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf");

pub struct Face {
    regular: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
    pub cell_w: u32,
    pub cell_h: u32,
    pub ascent: f64,
    cache: HashMap<(String, [u8; 3], bool, bool), Image>,
}

fn one(bytes: &[u8], path: Option<&std::path::Path>, px: f64) -> Result<Font, String> {
    match path {
        Some(path) => Font::load(path, px).or_else(|_| Font::from_bytes(bytes.to_vec(), px)),
        None => Font::from_bytes(bytes.to_vec(), px),
    }
}

impl Face {
    pub fn new(px: f64, custom: Option<&std::path::Path>) -> Result<Face, String> {
        let regular = one(REGULAR, custom, px)?;
        let bold = one(BOLD, None, px)?;
        let italic = one(ITALIC, None, px)?;
        let bold_italic = one(BOLD_ITALIC, None, px)?;
        let probe = regular.width_of("0000000000") / 10.0;
        let cell_w = probe.round().max(1.0) as u32;
        let cell_h = regular.line_height().round().max(1.0) as u32;
        let ascent = regular.ascent();
        Ok(Face { regular, bold, italic, bold_italic, cell_w, cell_h, ascent, cache: HashMap::new() })
    }

    fn pick(&self, bold: bool, italic: bool) -> &Font {
        match (bold, italic) {
            (true, true) => &self.bold_italic,
            (true, false) => &self.bold,
            (false, true) => &self.italic,
            (false, false) => &self.regular,
        }
    }

    pub fn glyph(&mut self, text: &str, fg: [u8; 3], bold: bool, italic: bool) -> &Image {
        let key = (text.to_string(), fg, bold, italic);
        if !self.cache.contains_key(&key) {
            let image = self.pick(bold, italic).paint(text, fg);
            self.cache.insert(key.clone(), image);
        }
        self.cache.get(&key).unwrap()
    }
}
