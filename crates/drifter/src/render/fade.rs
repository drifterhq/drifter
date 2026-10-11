use super::*;

pub(super) const ROUND_PATCH: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shadow {
    None,
    Tip,
    Menu,
    Modal,
}

impl Shadow {
    fn cast(self) -> Option<(f64, f64, f32)> {
        match self {
            Shadow::None => None,
            Shadow::Tip => Some((0.35, 3.0, 0.22)),
            Shadow::Menu => Some((0.6, 6.0, 0.30)),
            Shadow::Modal => Some((1.0, 8.0, 0.38)),
        }
    }
}

pub(super) fn round_art(renderer: &mut Painter, color: [u8; 3]) -> Option<TextureBuffer<Tex>> {
    let patch = drifter_core::paint::round_patch(ROUND_PATCH, color);
    upload_flat(renderer, Some(&patch), "round fill").ok().flatten()
}

impl State {
    pub(super) fn round_fill(
        &self,
        patch: &TextureBuffer<Tex>,
        radius: f64,
        rect: Rect,
        alpha: f32,
        elements: &mut Vec<Skin>,
    ) {
        if alpha <= 0.0 {
            return;
        }
        for (source, target) in drifter_core::window::round_slices(rect, ROUND_PATCH, radius) {
            elements.push(Skin::Texture(faded(patch, source, target, alpha)));
        }
    }

    pub(super) fn panel_shadow(&self, rect: Rect, kind: Shadow, alpha: f32, elements: &mut Vec<Skin>) {
        let (Some((zoom, drop, dark)), Some((buffer, corner))) = (kind.cast(), self.window_shadow.as_ref()) else {
            return;
        };
        if alpha <= 0.0 {
            return;
        }
        for (source, target) in drifter_core::window::shadow_slices(rect, *corner, zoom, drop) {
            elements.push(Skin::Texture(faded(buffer, source, target, dark * alpha)));
        }
    }

    pub(super) fn rebake_rounds(&mut self) {
        self.round = round_art(&mut self.renderer, self.palette.shade);
        self.accent_round = round_art(&mut self.renderer, self.look.accent);
    }
}
