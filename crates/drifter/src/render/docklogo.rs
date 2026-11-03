use super::*;

const GROW: f64 = 1.3;
const LIFT_SECONDS: f64 = 0.35;

pub(super) struct DockLogo {
    program: Option<smithay::backend::renderer::gles::GlesPixelProgram>,
    failed: bool,
    sheet: Option<(wallpaper::Sheet, (i32, i32))>,
    clock: (f64, Instant),
    frame: Option<(TextureBuffer<Tex>, i64)>,
    pub(super) hot: bool,
    pub(super) pace: f64,
    squircle: bool,
    lift: (f64, Instant),
    still: Option<(TextureBuffer<Tex>, (i32, i32))>,
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
            squircle: false,
            lift: (0.0, Instant::now()),
            still: None,
        }
    }
}

impl DockLogo {
    pub(super) fn squircle() -> DockLogo {
        DockLogo {
            squircle: true,
            ..DockLogo::default()
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
    if let Some(frame) = live_frame(logo, renderer, shown, hot, name) {
        return Some(frame);
    }
    if !logo.failed {
        return None;
    }
    let side = dense(shown.x.round() as u32) as i32;
    if let Some((frame, had)) = logo.still.as_ref()
        && *had == (side, side)
    {
        return Some(frame.clone());
    }
    let image = drifter_core::blob::still(side as u32, drifter_core::shader::hour_now(), logo.squircle);
    let frame = upload(renderer, Some(&image), name).ok().flatten()?;
    logo.still = Some((frame.clone(), (side, side)));
    Some(frame)
}

fn live_frame(logo: &mut DockLogo, renderer: &mut Painter, shown: Vec2, hot: bool, name: &str) -> Option<TextureBuffer<Tex>> {
    let now = Instant::now();
    let goal = match hot && logo.pace > 0.0 && logo.squircle {
        true => 1.0,
        false => 0.0,
    };
    let step = now.duration_since(logo.lift.1).as_secs_f64() / LIFT_SECONDS;
    let lift = match goal > logo.lift.0 {
        true => (logo.lift.0 + step).min(goal),
        false => (logo.lift.0 - step).max(goal),
    };
    logo.lift = (lift, now);
    let moving = (hot && logo.pace > 0.0) || lift > 0.0;
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
        logo.program = match logo.squircle {
            true => wallpaper::squircle_logo_program(renderer),
            false => wallpaper::logo_program(renderer),
        };
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
    let (then, at) = logo.clock;
    let clock = match moving {
        true => then + now.duration_since(at).as_secs_f64() * logo.pace,
        false => then,
    };
    logo.clock = (clock, now);
    let hour = drifter_core::shader::hour_now();
    let program = logo.program.clone()?;
    let (sheet, _) = logo.sheet.as_mut()?;
    let extra = logo.squircle.then_some([lift as f32, 0.0, 0.0, 0.0]);
    if let Err(error) = wallpaper::paint_logo(&program, renderer, sheet, size, clock, hour, extra) {
        log!("drifter: the {name} logo did not draw: {error}");
        logo.failed = true;
        return None;
    }
    let density = drifter_core::paint::density() as i32;
    let frame = TextureBuffer::from_texture(renderer, Tex::Gles(sheet.texture.clone()), density, Transform::Normal, None);
    logo.frame = Some((frame.clone(), minute));
    Some(frame)
}
