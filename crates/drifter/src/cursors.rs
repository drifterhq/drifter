use drifter_core::{Image, Vec2};

const FALLBACK_THEME: &str = "Adwaita";
pub fn load(names: &[&str], theme: Option<&str>, size: u32) -> Option<(Image, Vec2)> {
    let chosen = theme
        .map(str::to_string)
        .or_else(|| std::env::var("XCURSOR_THEME").ok())
        .unwrap_or_else(|| "default".to_string());
    let themes = [
        xcursor::CursorTheme::load(&chosen),
        xcursor::CursorTheme::load(FALLBACK_THEME),
    ];
    let path = themes
        .iter()
        .find_map(|theme| names.iter().find_map(|name| theme.load_icon(name)))?;
    let bytes = std::fs::read(&path).ok()?;
    let frames = xcursor::parser::parse_xcursor(&bytes)?;
    let want = size;
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
