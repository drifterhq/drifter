use super::*;
use drifter_core::sun::Sun;

const RIM_LENGTH: u32 = 256;
const SHEEN_SIZE: u32 = 128;

pub(super) struct Sunlight {
    pub(super) sun: Sun,
    key: Option<[i32; 5]>,
    along: Option<TextureBuffer<Tex>>,
    down: Option<TextureBuffer<Tex>>,
    glow: Option<TextureBuffer<Tex>>,
}

impl Default for Sunlight {
    fn default() -> Sunlight {
        Sunlight {
            sun: drifter_core::sun::sun(drifter_core::shader::hour_now()),
            key: None,
            along: None,
            down: None,
            glow: None,
        }
    }
}

fn key(sun: &Sun) -> [i32; 5] {
    let q = |v: f64| (v * 64.0).round() as i32;
    [q(sun.glint[0]), q(sun.glint[1]), q(sun.glint[2]), q(sun.rim), q(sun.side)]
}

impl State {
    pub(super) fn light_sun(&mut self) {
        let sun = drifter_core::sun::sun(drifter_core::shader::hour_now());
        self.sunlight.sun = sun;
        let now = key(&sun);
        if self.sunlight.key != Some(now) {
            let along = drifter_core::sun::rim_strip(&sun, RIM_LENGTH, true);
            let down = drifter_core::sun::rim_strip(&sun, RIM_LENGTH, false);
            self.sunlight.along = upload_flat(&mut self.renderer, Some(&along), "sun rim").ok().flatten();
            self.sunlight.down = upload_flat(&mut self.renderer, Some(&down), "sun side").ok().flatten();
            let glow = drifter_core::sun::sheen(&sun, SHEEN_SIZE);
            self.sunlight.glow = upload_flat(&mut self.renderer, Some(&glow), "sun sheen").ok().flatten();
            self.sunlight.key = Some(now);
        }
    }

    pub(super) fn sun_rim(&self, content: Rect, dressed: bool, fade: f32, elements: &mut Vec<Skin>) {
        let sun = &self.sunlight.sun;
        let edge = match dressed {
            true => self.chrome.layout(content).frame,
            false => content,
        };
        let frame = self.on_screen(edge);
        let zoom = self.camera.zoom;
        let inset = self.chrome.radius * zoom;
        let line = (2.0 * zoom).max(1.0);
        let alpha = fade * sun.rim as f32;
        if let Some(glow) = self.sunlight.glow.as_ref() {
            let whole = Rect::new(Vec2::ZERO, Vec2::splat(f64::from(SHEEN_SIZE)));
            elements.push(Skin::Texture(faded(glow, whole, frame, fade * sun.sheen as f32)));
        }
        if let Some(along) = self.sunlight.along.as_ref() {
            let top = Rect::new(
                Vec2::new(frame.origin.x + inset * 0.6, frame.origin.y),
                Vec2::new((frame.size.x - inset * 1.2).max(0.0), line),
            );
            let whole = Rect::new(Vec2::ZERO, Vec2::new(f64::from(RIM_LENGTH), 1.0));
            elements.push(Skin::Texture(faded(along, whole, top, alpha)));
        }
        if let Some(down) = self.sunlight.down.as_ref()
            && sun.side.abs() > 0.15
        {
            let x = match sun.side < 0.0 {
                true => frame.origin.x,
                false => frame.origin.x + frame.size.x - line,
            };
            let side = Rect::new(
                Vec2::new(x, frame.origin.y + inset * 0.6),
                Vec2::new(line, (frame.size.y * 0.7 - inset).max(0.0)),
            );
            let whole = Rect::new(Vec2::ZERO, Vec2::new(1.0, f64::from(RIM_LENGTH)));
            let lean = ((sun.side.abs() - 0.15) / 0.85).clamp(0.0, 1.0) as f32;
            elements.push(Skin::Texture(faded(down, whole, side, alpha * lean)));
        }
    }
}
