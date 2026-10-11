use crate::geom::Vec2;

pub const SPAN_MS: u64 = 380;

pub const GENIE_FRAG: &str = "#version 100
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision mediump float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
varying vec2 v_coords;
uniform vec4 win;
uniform vec4 slot;
uniform float t;
void main() {
  vec2 p = v_coords;
  float bend = smoothstep(0.0, 0.45, t);
  float slide = smoothstep(0.25, 1.0, t);
  float top = mix(win.y, slot.y, slide);
  float bot = mix(win.y + win.w, slot.y + slot.w, slide);
  float k = clamp((p.y - win.y) / max(slot.y + slot.w - win.y, 1e-4), 0.0, 1.0);
  float c = 0.5 - 0.5 * cos(3.14159 * k);
  float l = mix(win.x, mix(win.x, slot.x, c), bend);
  float r = mix(win.x + win.z, mix(win.x + win.z, slot.x + slot.z, c), bend);
  vec2 uv = vec2((p.x - l) / (r - l), (p.y - top) / (bot - top));
  if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) discard;
  gl_FragColor = texture2D(tex, uv) * alpha;
}
";

fn smoothstep(lo: f64, hi: f64, x: f64) -> f64 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn source_of(p: Vec2, win: [f64; 4], slot: [f64; 4], t: f64) -> Option<Vec2> {
    let bend = smoothstep(0.0, 0.45, t);
    let slide = smoothstep(0.25, 1.0, t);
    let top = mix(win[1], slot[1], slide);
    let bot = mix(win[1] + win[3], slot[1] + slot[3], slide);
    let k = ((p.y - win[1]) / (slot[1] + slot[3] - win[1]).max(1e-4)).clamp(0.0, 1.0);
    let c = 0.5 - 0.5 * (std::f64::consts::PI * k).cos();
    let l = mix(win[0], mix(win[0], slot[0], c), bend);
    let r = mix(win[0] + win[2], mix(win[0] + win[2], slot[0] + slot[2], c), bend);
    let uv = Vec2::new((p.x - l) / (r - l), (p.y - top) / (bot - top));
    (uv.x >= 0.0 && uv.y >= 0.0 && uv.x <= 1.0 && uv.y <= 1.0).then_some(uv)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: [f64; 4] = [0.1, 0.0, 0.6, 0.7];
    const SLOT: [f64; 4] = [0.7, 0.94, 0.05, 0.06];

    #[test]
    fn genie_at_rest_is_the_window() {
        let p = Vec2::new(0.4, 0.35);
        let uv = source_of(p, WIN, SLOT, 0.0).expect("inside");
        assert!((uv.x - 0.5).abs() < 1e-9 && (uv.y - 0.5).abs() < 1e-9);
        assert_eq!(source_of(Vec2::new(0.05, 0.35), WIN, SLOT, 0.0), None, "nothing outside the window");
    }

    #[test]
    fn genie_ends_inside_the_slot() {
        let middle = Vec2::new(SLOT[0] + SLOT[2] / 2.0, SLOT[1] + SLOT[3] / 2.0);
        assert!(source_of(middle, WIN, SLOT, 1.0).is_some());
        assert_eq!(source_of(Vec2::new(0.4, 0.35), WIN, SLOT, 1.0), None, "the old spot is empty");
    }

    #[test]
    fn genie_narrows_toward_the_dock() {
        let wide = |y: f64| {
            (0..1000)
                .filter(|x| source_of(Vec2::new(f64::from(*x) / 1000.0, y), WIN, SLOT, 0.5).is_some())
                .count()
        };
        let high = wide(0.3);
        let low = wide(0.8);
        assert!(high > 0 && low < high, "{high} then {low}");
    }

    #[test]
    fn the_genie_shader_names_its_inputs() {
        for name in ["//_DEFINES_", "EXTERNAL", "tex", "v_coords", "alpha", "uniform vec4 win", "uniform vec4 slot", "uniform float t"] {
            assert!(GENIE_FRAG.contains(name), "{name}");
        }
    }
}
