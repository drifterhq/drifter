use super::*;
use drifter_core::motion::{Motion, Phase};
use smithay::backend::renderer::utils::RendererSurfaceStateUserData;

pub(super) struct Ghost {
    client: Option<(TextureBuffer<Tex>, Vec2, f32)>,
    content: Rect,
    inset: Vec2,
    dressed: bool,
    corners: [f64; 4],
    maximized: bool,
    title: String,
    motion: Motion,
}

impl State {
    pub(super) fn remember_surfaces(&mut self) {
        let context = smithay::backend::renderer::Renderer::context_id(&self.renderer);
        self.kept.retain(|id, _| self.windows.iter().any(|window| window.id == *id));
        for window in &self.windows {
            if !window.placed || window.hidden {
                continue;
            }
            let kept = smithay::wayland::compositor::with_states(window.toplevel.wl_surface(), |states| {
                let data = states.data_map.get::<RendererSurfaceStateUserData>()?;
                let data = data.lock().ok()?;
                let size = data.surface_size()?;
                let texture = data.texture::<Tex>(context.clone())?.clone();
                drop(data);
                let alpha = states
                    .cached_state
                    .get::<smithay::wayland::alpha_modifier::AlphaModifierSurfaceCachedState>()
                    .current()
                    .multiplier_f32()
                    .unwrap_or(1.0);
                Some((texture, Vec2::new(f64::from(size.w), f64::from(size.h)), alpha))
            });
            if let Some(kept) = kept {
                self.kept.insert(window.id, kept);
            }
        }
    }

    pub(super) fn keep_ghost(&mut self, index: usize) {
        let Some(window) = self.windows.get(index) else {
            return;
        };
        if !window.placed || window.hidden || window.closed_at.is_some() || self.window_pace <= 0.0 {
            return;
        }
        let motion = match window.motion {
            Some(motion) if motion.phase == Phase::Closing => motion,
            _ => Motion::new(Phase::Closing, window.content),
        };
        let client = self.kept.remove(&window.id).map(|(texture, size, alpha)| {
            (TextureBuffer::from_texture(&self.renderer, texture, 1, Transform::Normal, None), size, alpha)
        });
        self.ghosts.push(Ghost {
            client,
            content: window.content,
            inset: window.inset,
            dressed: window.decorated && window.full.is_none(),
            corners: self.corners_of(window),
            maximized: window.restore.is_some(),
            title: window.title.clone(),
            motion,
        });
    }

    pub(super) fn step_ghosts(&mut self, dt: Duration) {
        let pace = self.window_pace;
        for ghost in &mut self.ghosts {
            ghost.motion.advance(match pace {
                pace if pace <= 0.0 => Duration::from_secs(60),
                pace => dt.mul_f64(pace),
            });
        }
        self.ghosts.retain(|ghost| !ghost.motion.done());
    }

    pub(super) fn ghost_elements(&self, elements: &mut Vec<Skin>) {
        for ghost in &self.ghosts {
            let (shown, fade) = ghost.motion.shape_for(ghost.content, self.window_still);
            if fade <= 0.0 {
                continue;
            }
            if ghost.dressed {
                self.dress(shown, (0.0, ghost.maximized), &ghost.title, fade, elements);
            }
            if let Some((art, size, alpha)) = ghost.client.as_ref() {
                let stretch = match ghost.content.size.x > 0.0 {
                    true => shown.size.x / ghost.content.size.x,
                    false => 1.0,
                };
                let at = Rect::new(shown.origin - ghost.inset * stretch, *size * stretch);
                elements.push(Skin::Texture(element(art, self.on_screen(at), fade * alpha)));
            }
            if !ghost.maximized {
                self.cast_shadow(shown, ghost.dressed, ghost.corners, 0.0, fade, elements);
            }
        }
    }
}
