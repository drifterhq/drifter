use super::*;
use drifter_core::motion::Phase;
use smithay::backend::renderer::gles::{GlesTexture, Uniform, UniformName, UniformType};
use smithay::backend::renderer::{Bind, Frame, Renderer};

pub(super) struct Stow {
    art: GlesTexture,
    size: (i32, i32),
    frame: Rect,
    sheet: Option<(super::wallpaper::Sheet, (i32, i32))>,
}

impl State {
    fn genie_ready(&mut self) -> bool {
        if self.minimize_effect != drifter_core::Minimize::Genie
            || self.window_still
            || self.window_pace <= 0.0
            || self.renderer.is_pixman()
        {
            return false;
        }
        if self.genie.is_none() {
            let Painter::Gles(gles) = &mut self.renderer else {
                return false;
            };
            let names = [
                UniformName::new("win", UniformType::_4f),
                UniformName::new("slot", UniformType::_4f),
                UniformName::new("t", UniformType::_1f),
            ];
            self.genie = Some(match gles.compile_custom_texture_shader(drifter_core::genie::GENIE_FRAG, &names) {
                Ok(program) => Some(program),
                Err(error) => {
                    log!("drifter: the genie shader does not compile: {error}");
                    None
                }
            });
        }
        matches!(self.genie, Some(Some(_)))
    }

    pub(super) fn begin_stow(&mut self, index: usize) {
        let Some(id) = self.windows.get(index).map(|window| window.id) else {
            return;
        };
        self.stows.remove(&id);
        if !self.genie_ready() {
            return;
        }
        let Some(stow) = self.snapshot_window(index) else {
            return;
        };
        if let Some(motion) = self.windows[index].motion.as_mut() {
            motion.span = Duration::from_millis(drifter_core::genie::SPAN_MS);
        }
        self.stows.insert(id, stow);
    }

    fn snapshot_window(&mut self, index: usize) -> Option<Stow> {
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};
        use smithay::utils::{Buffer as BufferCoord, Physical, Size};

        let window = self.windows.get(index)?;
        let item = self.showing_of(window);
        let frame = match item.5 > 0.0 {
            true => self.chrome.layout(window.content).frame,
            false => window.content,
        };
        let still = (item.0, item.0, 1.0, item.3, item.4, item.5, item.6, item.7);
        let scale = self.heads.get(self.at).map_or(1.0, |head| head.scale);
        let mut skins = Vec::new();
        self.show_window(still, false, &mut skins);
        let shown_at = self.on_screen(frame);
        let origin = (shown_at.origin * scale).round();
        let far = ((shown_at.origin + shown_at.size) * scale).round();
        let (w, h) = ((far.x - origin.x).max(1.0) as i32, (far.y - origin.y).max(1.0) as i32);
        let shown: Vec<RelocateRenderElement<Shown>> = skins
            .into_iter()
            .map(|skin| {
                RelocateRenderElement::from_element(
                    RescaleRenderElement::from_element(skin, (0, 0).into(), scale),
                    (-(origin.x as i32), -(origin.y as i32)),
                    Relocate::Relative,
                )
            })
            .collect();
        let mut canvas = self.renderer.canvas(DrmFourcc::Abgr8888, Size::<i32, BufferCoord>::from((w, h))).ok()?;
        {
            let mut target = self.renderer.bind(&mut canvas).ok()?;
            let mut tracker = OutputDamageTracker::new(Size::<i32, Physical>::from((w, h)), 1.0, Transform::Normal);
            tracker
                .render_output(&mut self.renderer, &mut target, 0, &shown, Color32F::TRANSPARENT)
                .ok()?;
        }
        match canvas {
            crate::paint::Canvas::Gles(art) => Some(Stow {
                art,
                size: (w, h),
                frame,
                sheet: None,
            }),
            _ => None,
        }
    }

    pub(super) fn in_genie(&self, window: &crate::shell::Window) -> bool {
        self.stows.contains_key(&window.id)
            && window
                .motion
                .is_some_and(|motion| matches!(motion.phase, Phase::Minimizing | Phase::Restoring))
    }

    pub(super) fn genie_elements(&mut self, elements: &mut Vec<Skin>) {
        let live: Vec<(u64, drifter_core::motion::Motion)> = self
            .windows
            .iter()
            .filter(|window| self.in_genie(window))
            .filter_map(|window| Some((window.id, window.motion?)))
            .collect();
        self.stows.retain(|id, _| live.iter().any(|(held, _)| held == id));
        let scale = self.heads.get(self.at).map_or(1.0, |head| head.scale);
        for (id, motion) in live {
            let Some(stow) = self.stows.get(&id) else {
                continue;
            };
            let frame = self.on_screen(stow.frame);
            let slot = self.on_screen(motion.dock);
            let origin = frame.origin.min(slot.origin);
            let reach = frame.max().max(slot.max());
            let area = Rect::new(origin, reach - origin);
            let local = |rect: Rect| {
                [
                    ((rect.origin.x - area.origin.x) / area.size.x) as f32,
                    ((rect.origin.y - area.origin.y) / area.size.y) as f32,
                    (rect.size.x / area.size.x) as f32,
                    (rect.size.y / area.size.y) as f32,
                ]
            };
            let progress = motion.progress();
            let (t, alpha) = match motion.phase {
                Phase::Restoring => (1.0 - progress, 1.0 - (1.0 - progress) * (1.0 - progress)),
                _ => (progress, 1.0 - progress * progress),
            };
            let size = ((area.size.x * scale).round().max(1.0) as i32, (area.size.y * scale).round().max(1.0) as i32);
            let (win, seat) = (local(frame), local(slot));
            let Some(stow) = self.stows.get_mut(&id) else {
                continue;
            };
            if stow.sheet.as_ref().is_none_or(|(_, held)| *held != size) {
                stow.sheet = super::wallpaper::sheet(&mut self.renderer, size).ok().map(|sheet| (sheet, size));
            }
            let (Some(Some(program)), Some((sheet, _)), Painter::Gles(gles)) =
                (self.genie.as_ref(), stow.sheet.as_mut(), &mut self.renderer)
            else {
                continue;
            };
            let whole = smithay::utils::Rectangle::from_size(size.into());
            let uniforms = [
                Uniform::new("win", (win[0], win[1], win[2], win[3])),
                Uniform::new("slot", (seat[0], seat[1], seat[2], seat[3])),
                Uniform::new("t", t as f32),
            ];
            let painted = (|| -> Result<(), String> {
                let mut target = gles.bind(&mut sheet.texture).map_err(|e| e.to_string())?;
                let mut frame = gles.render(&mut target, size.into(), Transform::Normal).map_err(|e| e.to_string())?;
                frame.clear(Color32F::TRANSPARENT, &[whole]).map_err(|e| e.to_string())?;
                frame
                    .render_texture_from_to(
                        &stow.art,
                        smithay::utils::Rectangle::from_size((f64::from(stow.size.0), f64::from(stow.size.1)).into()),
                        whole,
                        &[whole],
                        &[],
                        Transform::Normal,
                        1.0,
                        Some(program),
                        &uniforms,
                    )
                    .map_err(|e| e.to_string())?;
                let _ = frame.finish().map_err(|e| e.to_string())?;
                Ok(())
            })();
            match painted {
                Ok(()) => {
                    sheet.renew(&self.renderer);
                    elements.push(Skin::Texture(element(&sheet.buffer, area, alpha as f32)));
                }
                Err(error) => log!("drifter: no genie: {error}"),
            }
        }
    }
}
