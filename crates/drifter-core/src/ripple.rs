pub const SECONDS: f64 = 1.4;
pub const REACH: f64 = 300.0;
pub const SPEED: f64 = 260.0;
pub const STRENGTH: (u32, u32) = (25, 200);

const WAVELENGTH: f64 = 40.0;
const WIDTH: f64 = 30.0;
const DAMP: f64 = 1.6;
const BEND: f64 = 50.0;
const SHADE: f64 = 1.5;
const SPONGE: f64 = 90.0;

pub fn source() -> String {
    let freq = std::f64::consts::TAU / WAVELENGTH;
    format!(
        "#version 100
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
uniform float K;
uniform float G;
void main() {{
  vec2 p = v_coords * S;
  vec2 d = p - O;
  float r = length(d);
  float x = r - K * {SPEED:.1};
  float w = exp(-x * x / {two_width:.1});
  float fade = exp(-K * {DAMP:.3}) * G * clamp(({REACH:.1} - r) / {SPONGE:.1}, 0.0, 1.0);
  float slope = ({freq:.5} * cos(x * {freq:.5}) - x / {width_sq:.1} * sin(x * {freq:.5})) * w * fade;
  vec2 dir = r > 0.5 ? d / r : vec2(0.0);
  vec2 bend = dir * slope * {BEND:.1};
  vec4 c = vec4(0.0, 0.0, 0.0, 1.0);
  c.r = texture2D(tex, (p + bend * 0.94) / S).r;
  c.g = texture2D(tex, (p + bend) / S).g;
  c.b = texture2D(tex, (p + bend * 1.07) / S).b;
  c.rgb *= 1.0 + slope * {SHADE:.2};
  gl_FragColor = c * alpha;
}}
",
        two_width = 2.0 * WIDTH * WIDTH,
        width_sq = WIDTH * WIDTH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_reaches_its_edge_and_stays_near_the_click() {
        assert!(SPEED * SECONDS >= REACH, "{} < {REACH}", SPEED * SECONDS);
        assert!(SPEED * SECONDS <= REACH * 1.5, "{} > {REACH} * 1.5", SPEED * SECONDS);
        assert!(source().contains(&format!("{REACH:.1} - r")));
    }

    #[test]
    fn the_ripple_shader_names_its_inputs() {
        let source = source();
        for name in ["//_DEFINES_", "EXTERNAL", "tex", "v_coords", "alpha", "uniform vec2 S", "uniform vec2 O", "uniform float K", "uniform float G"] {
            assert!(source.contains(name), "{name}");
        }
        assert_eq!(source.matches('{').count(), source.matches('}').count());
        assert!(!source.contains("NaN") && !source.contains("inf"));
    }
}
