use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

pub fn ui(px: f64) -> Result<Font, String> {
    Font::find(None, px).or_else(|_| Font::from_bytes(REGULAR.to_vec(), px))
}

const STYLES: [&str; 4] = ["Regular", "Bold", "Italic", "Bold Italic"];

static FOUND: Mutex<Option<(String, [Option<PathBuf>; 4])>> = Mutex::new(None);

fn fc_match(family: &str, style: &str) -> Option<PathBuf> {
    let pattern = format!("{family}:style={style}");
    let out = std::process::Command::new("fc-match").args(["-f", "%{family}\n%{file}", &pattern]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (families, file) = text.split_once('\n')?;
    let wanted = family.to_lowercase();
    let same = families.split(',').any(|each| each.trim().to_lowercase() == wanted);
    let usable = file.ends_with(".ttf") || file.ends_with(".otf") || file.ends_with(".ttc");
    (same && usable).then(|| PathBuf::from(file))
}

fn files(family: &str) -> [Option<PathBuf>; 4] {
    if family.starts_with('/') {
        return [Some(PathBuf::from(family)), None, None, None];
    }
    let mut found = FOUND.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((name, files)) = found.as_ref()
        && name == family
    {
        return files.clone();
    }
    let files = STYLES.map(|style| fc_match(family, style));
    *found = Some((family.to_string(), files.clone()));
    files
}

fn one(bytes: &[u8], path: Option<&Path>, px: f64) -> Result<Font, String> {
    match path {
        Some(path) => Font::load(path, px).or_else(|_| Font::from_bytes(bytes.to_vec(), px)),
        None => Font::from_bytes(bytes.to_vec(), px),
    }
}

impl Face {
    pub fn new(px: f64, family: Option<&str>) -> Result<Face, String> {
        let [r, b, i, bi] = family.map(files).unwrap_or_default();
        if let Some(name) = family
            && r.is_none()
        {
            log!("drifter-terminal: font {name} is not installed, using JetBrains Mono");
        }
        let custom = r.is_some();
        let pick = |own: &Option<PathBuf>, built: &'static [u8]| match (own, custom) {
            (Some(path), _) => one(built, Some(path), px),
            (None, true) => one(built, r.as_deref(), px),
            (None, false) => one(built, None, px),
        };
        let regular = pick(&r, REGULAR)?;
        let bold = pick(&b, BOLD)?;
        let italic = pick(&i, ITALIC)?;
        let bold_italic = pick(&bi, BOLD_ITALIC)?;
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
