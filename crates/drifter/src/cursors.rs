use drifter_core::{Image, Vec2};

const FALLBACK_THEME: &str = "Adwaita";
const SIZE: u32 = 24;

pub fn size() -> u32 {
    std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|text| text.parse().ok())
        .filter(|size| *size > 0)
        .unwrap_or(SIZE)
}

pub fn load(names: &[&str]) -> Option<(Image, Vec2)> {
    let chosen = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".to_string());
    let themes = [
        xcursor::CursorTheme::load(&chosen),
        xcursor::CursorTheme::load(FALLBACK_THEME),
    ];
    let path = themes
        .iter()
        .find_map(|theme| names.iter().find_map(|name| theme.load_icon(name)))?;
    let bytes = std::fs::read(&path).ok()?;
    let frames = xcursor::parser::parse_xcursor(&bytes)?;
    let want = size();
    let best = frames
        .iter()
        .map(|frame| frame.size)
        .min_by_key(|size| size.abs_diff(want))?;
    let frame = frames.into_iter().find(|frame| frame.size == best)?;
    let image = Image {
        width: frame.width,
        height: frame.height,
        rgba: frame.pixels_rgba,
    };
    if image.rgba.len() != (image.width as usize) * (image.height as usize) * 4 {
        return None;
    }
    let hotspot = Vec2::new(f64::from(frame.xhot), f64::from(frame.yhot));
    Some((image, hotspot))
}
