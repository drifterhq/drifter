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
        let moving = hot && self.dock_logo.pace > 0.0;
        let was_moving = std::mem::replace(&mut self.dock_logo.hot, moving);
        let minute = (drifter_core::shader::hour_now() * 60.0) as i64;
        let side = dense(shown.size.x.round() as u32) as i32;
        let size = (side, side);
        let fresh = self.dock_logo.sheet.as_ref().is_some_and(|(_, had)| *had == size);
        if !moving
            && !was_moving
            && fresh
            && let Some((frame, at)) = self.dock_logo.frame.as_ref()
            && *at == minute
        {
            return Some((frame.clone(), shown));
        }
        if self.dock_logo.failed {
            return None;
        }
        if self.dock_logo.program.is_none() {
            self.dock_logo.program = wallpaper::logo_program(&mut self.renderer);
            if self.dock_logo.program.is_none() {
                self.dock_logo.failed = true;
                return None;
            }
        }
        if !fresh {
            match wallpaper::sheet(&mut self.renderer, size) {
                Ok(sheet) => self.dock_logo.sheet = Some((sheet, size)),
                Err(error) => {
                    log!("drifter: the dock logo has no texture: {error}");
                    self.dock_logo.failed = true;
                    return None;
                }
            }
        }
        let now = Instant::now();
        let (then, at) = self.dock_logo.clock;
        let clock = match moving {
            true => then + now.duration_since(at).as_secs_f64() * self.dock_logo.pace,
            false => then,
        };
        self.dock_logo.clock = (clock, now);
        let hour = drifter_core::shader::hour_now();
        let program = self.dock_logo.program.clone()?;
        let (sheet, _) = self.dock_logo.sheet.as_mut()?;
        if let Err(error) = wallpaper::paint_logo(&program, &mut self.renderer, sheet, size, clock, hour, None) {
            log!("drifter: the dock logo did not draw: {error}");
            self.dock_logo.failed = true;
            return None;
        }
        let density = drifter_core::paint::density() as i32;
        let frame = TextureBuffer::from_texture(&self.renderer, Tex::Gles(sheet.texture.clone()), density, Transform::Normal, None);
        self.dock_logo.frame = Some((frame.clone(), minute));
        Some((frame, shown))
    }
}
