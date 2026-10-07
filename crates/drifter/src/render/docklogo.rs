use super::*;

const GROW: f64 = 1.3;

pub(super) struct DockLogo {
    program: Option<smithay::backend::renderer::gles::GlesPixelProgram>,
    failed: bool,
    sheet: Option<(wallpaper::Sheet, (i32, i32))>,
    clock: (f64, Instant),
    frame: Option<(TextureBuffer<Tex>, i64)>,
    pub(super) hot: bool,
    pub(super) pace: f64,
}

impl Default for DockLogo {
    fn default() -> DockLogo {
        DockLogo {
            program: None,
            failed: false,
            sheet: None,
            clock: (20.0, Instant::now()),
            frame: None,
            hot: false,
            pace: drifter_core::welcome::LOGO_PACE,
        }
    }
}

impl State {
    pub(super) fn dock_logo_frame(&mut self, rect: Rect, hot: bool) -> Option<(TextureBuffer<Tex>, Rect)> {
        let shown = Rect::from_center(rect.center(), rect.size * GROW);
        let frame = logo_frame(&mut self.dock_logo, &mut self.renderer, shown.size, hot, "dock")?;
        Some((frame, shown))
    }

    pub(super) fn lock_logo_frame(&mut self, rect: Rect) -> Option<TextureBuffer<Tex>> {
        logo_frame(&mut self.lock_logo, &mut self.renderer, rect.size, true, "lock")
    }
}

fn logo_frame(logo: &mut DockLogo, renderer: &mut Painter, shown: Vec2, hot: bool, name: &str) -> Option<TextureBuffer<Tex>> {
    let moving = hot && logo.pace > 0.0;
    let was_moving = std::mem::replace(&mut logo.hot, moving);
    if moving && !was_moving {
        logo.clock.1 = Instant::now();
    }
    let minute = (drifter_core::shader::hour_now() * 60.0) as i64;
    let side = dense(shown.x.round() as u32) as i32;
    let size = (side, side);
    let fresh = logo.sheet.as_ref().is_some_and(|(_, had)| *had == size);
    if !moving
        && !was_moving
        && fresh
        && let Some((frame, at)) = logo.frame.as_ref()
        && *at == minute
    {
        return Some(frame.clone());
    }
    if logo.failed {
        return None;
    }
    if logo.program.is_none() {
        logo.program = wallpaper::logo_program(renderer);
        if logo.program.is_none() {
            logo.failed = true;
            return None;
        }
    }
    if !fresh {
        match wallpaper::sheet(renderer, size) {
            Ok(sheet) => logo.sheet = Some((sheet, size)),
            Err(error) => {
                log!("drifter: the {name} logo has no texture: {error}");
                logo.failed = true;
                return None;
            }
        }
    }
    let now = Instant::now();
    let (then, at) = logo.clock;
    let clock = match moving {
        true => then + now.duration_since(at).as_secs_f64() * logo.pace,
        false => then,
    };
    logo.clock = (clock, now);
    let hour = drifter_core::shader::hour_now();
    let program = logo.program.clone()?;
    let (sheet, _) = logo.sheet.as_mut()?;
    if let Err(error) = wallpaper::paint_logo(&program, renderer, sheet, size, clock, hour, None) {
        log!("drifter: the {name} logo did not draw: {error}");
        logo.failed = true;
        return None;
    }
    let density = drifter_core::paint::density() as i32;
    let frame = TextureBuffer::from_texture(renderer, Tex::Gles(sheet.texture.clone()), density, Transform::Normal, None);
    logo.frame = Some((frame.clone(), minute));
    Some(frame)
}
