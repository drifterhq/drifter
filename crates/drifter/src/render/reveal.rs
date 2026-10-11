use super::*;
use smithay::backend::renderer::gles::{GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType};
use smithay::backend::renderer::{Bind, Frame, Renderer};

pub(super) const REVEAL: Duration = Duration::from_millis(600);
const FEATHER: f64 = 24.0;

const SOURCE: &str = "#version 100
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
varying vec2 v_coords;
uniform vec2 S;
uniform vec2 O;
uniform float Q;
uniform float F;
void main() {
  float keep = smoothstep(Q - F, Q + F, length(v_coords * S - O));
  gl_FragColor = texture2D(tex, v_coords) * alpha * keep;
}
";

#[allow(clippy::too_many_arguments)]
fn masked(
    program: &GlesTexProgram,
    renderer: &mut Painter,
    slot: &mut Option<(super::wallpaper::Sheet, (i32, i32))>,
    shot: &GlesTexture,
    size: (i32, i32),
    screen: Vec2,
    from: Vec2,
    t: f64,
) -> Result<(), String> {
    if slot.as_ref().is_none_or(|(_, held)| *held != size) {
        *slot = Some((super::wallpaper::sheet(renderer, size)?, size));
    }
    let (Some((sheet, _)), Painter::Gles(gles)) = (slot.as_mut(), &mut *renderer) else {
        return Err("a reveal needs the gles renderer".to_string());
    };
    let radius = drifter_core::motion::reveal_radius(from, screen, t, FEATHER);
    let area = smithay::utils::Rectangle::from_size(size.into());
    let uniforms = [
        Uniform::new("S", (screen.x as f32, screen.y as f32)),
        Uniform::new("O", (from.x as f32, from.y as f32)),
        Uniform::new("Q", radius as f32),
        Uniform::new("F", FEATHER as f32),
    ];
    let mut target = gles.bind(&mut sheet.texture).map_err(|e| e.to_string())?;
    let mut frame = gles.render(&mut target, size.into(), Transform::Normal).map_err(|e| e.to_string())?;
    frame.clear(Color32F::TRANSPARENT, &[area]).map_err(|e| e.to_string())?;
    frame
        .render_texture_from_to(
            shot,
            smithay::utils::Rectangle::from_size((f64::from(size.0), f64::from(size.1)).into()),
            area,
            &[area],
            &[],
            Transform::Normal,
            1.0,
            Some(program),
            &uniforms,
        )
        .map_err(|e| e.to_string())?;
    let _ = frame.finish().map_err(|e| e.to_string())?;
    drop(target);
    sheet.renew(renderer);
    Ok(())
}

pub(super) struct Reveal {
    program: GlesTexProgram,
    sheet: Option<(super::wallpaper::Sheet, (i32, i32))>,
}

impl State {
    pub(super) fn reveals(&mut self) -> bool {
        if self.transition != drifter_core::Transition::Reveal || self.window_still || self.renderer.is_pixman() {
            return false;
        }
        if self.reveal.is_none() {
            let Painter::Gles(gles) = &mut self.renderer else {
                return false;
            };
            let names = [
                UniformName::new("S", UniformType::_2f),
                UniformName::new("O", UniformType::_2f),
                UniformName::new("Q", UniformType::_1f),
                UniformName::new("F", UniformType::_1f),
            ];
            match gles.compile_custom_texture_shader(SOURCE, &names) {
                Ok(program) => self.reveal = Some(Some(Reveal { program, sheet: None })),
                Err(error) => {
                    log!("drifter: the reveal shader does not compile: {error}");
                    self.reveal = Some(None);
                }
            }
        }
        matches!(self.reveal, Some(Some(_)))
    }

    pub(super) fn paint_reveal(&mut self) {
        let Some(since) = self.style_fade else {
            return;
        };
        let Some((_, shot)) = self.style_tex.iter().find(|(head, _)| *head == self.at) else {
            return;
        };
        let shot: GlesTexture = shot.clone();
        let whole = self.physical();
        let size = (whole.x as i32, whole.y as i32);
        let t = since.elapsed().as_secs_f64() / REVEAL.as_secs_f64();
        let from = self.reveal_from.unwrap_or(self.screen / 2.0);
        let screen = self.screen;
        let Some(Some(reveal)) = self.reveal.as_mut() else {
            return;
        };
        if let Err(error) = masked(&reveal.program, &mut self.renderer, &mut reveal.sheet, &shot, size, screen, from, t) {
            log!("drifter: no reveal: {error}");
            self.style_tex.clear();
        }
    }

    pub(super) fn shoot_paper_for_reveal(&mut self, old: &mut Backdrop) {
        let from = self.paper_from.take().unwrap_or(self.screen / 2.0);
        if !self.reveals() {
            return;
        }
        if let Some((shot, size)) = self.shoot_paper(old) {
            self.paper_reveal = Some((shot, size, from, Instant::now()));
        }
    }

    pub(super) fn paper_reveal_elements(&mut self, elements: &mut Vec<Skin>) -> bool {
        let Some((shot, size, from, since)) = self.paper_reveal.clone() else {
            return false;
        };
        let t = since.elapsed().as_secs_f64() / REVEAL.as_secs_f64();
        if t >= 1.0 {
            self.paper_reveal = None;
            return false;
        }
        let screen = self.screen;
        let Some(Some(reveal)) = self.reveal.as_mut() else {
            self.paper_reveal = None;
            return false;
        };
        let mut slot = self.paper_sheet.take();
        let done = masked(&reveal.program, &mut self.renderer, &mut slot, &shot, size, screen, from, t);
        self.paper_sheet = slot;
        match (done, self.paper_sheet.as_ref()) {
            (Ok(()), Some((sheet, _))) => {
                elements.push(Skin::Texture(element(&sheet.buffer, Rect::new(Vec2::ZERO, screen), 1.0)));
                true
            }
            (Err(error), _) => {
                log!("drifter: no wallpaper reveal: {error}");
                self.paper_reveal = None;
                false
            }
            _ => false,
        }
    }

    pub(super) fn reveal_sheet(&self) -> Option<&TextureBuffer<Tex>> {
        self.style_tex.iter().any(|(head, _)| *head == self.at).then_some(())?;
        let Some(Some(reveal)) = self.reveal.as_ref() else {
            return None;
        };
        reveal.sheet.as_ref().map(|(sheet, _)| &sheet.buffer)
    }
}
