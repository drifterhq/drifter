pub const SECONDS: f64 = 1.4;
pub const REACH: f64 = 300.0;
pub const SPEED: f64 = 260.0;
pub const STRENGTH: (u32, u32) = (25, 200);
pub const DROPS: usize = 16;

const WAVELENGTH: f64 = 40.0;
const WIDTH: f64 = 30.0;
const DAMP: f64 = 1.6;
const SPONGE: f64 = 200.0;
const GAIN: f64 = 2.0;
const BEND: f64 = 85.0;
const SHADE: f64 = 0.4;
const CURVE: f64 = 2.0;
const SPLIT: f64 = 0.1;
const SWIRL: f64 = 120.0;
const GLINT: f64 = 0.45;
const SHINE: f64 = 60.0;
const TILT: f64 = 9.0;
const LIGHT_ANGLE: f64 = 130.0;
const LIGHT_HEIGHT: f64 = 40.0;
const GLINT_SPLIT: f64 = 0.4;

pub fn drop_name(index: usize) -> String {
    format!("D{index}")
}

pub fn source() -> String {
    let freq = std::f64::consts::TAU / WAVELENGTH;
    let (angle, height) = (LIGHT_ANGLE.to_radians(), LIGHT_HEIGHT.to_radians());
    let light = (angle.cos() * height.cos(), angle.sin() * height.cos(), height.sin());
    let uniforms: String = (0..DROPS).map(|i| format!("uniform vec3 {};\n", drop_name(i))).collect();
    let drops: String = (0..DROPS).map(|i| format!("  wave(p, {}, g, sw, rs, ss, ew, aw);\n", drop_name(i))).collect();
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
uniform float G;
{uniforms}
void wave(vec2 p, vec3 D, inout vec2 g, inout vec2 sw, inout vec2 rs, inout float ss, inout float ew, inout float aw) {{
  if (D.z < 0.0) return;
  vec2 d = p - D.xy;
  float r = length(d);
  float x = r - D.z * {SPEED:.1};
  float w = exp(-x * x / {two_width:.1});
  float s = ({freq:.5} * cos(x * {freq:.5}) - x / {width_sq:.1} * sin(x * {freq:.5})) * w;
  s *= exp(-D.z * {DAMP:.3}) * G * {GAIN:.2} * clamp(({REACH:.1} - r) / {SPONGE:.1}, 0.0, 1.0);
  vec2 dir = r > 0.5 ? d / r : vec2(0.0);
  float e = pow(clamp(r / {REACH:.1}, 0.0, 1.0), {CURVE:.2});
  g += dir * s;
  ss += s;
  sw += vec2(-dir.y, dir.x) * abs(s) * e;
  rs += dir * s * e;
  ew += abs(s) * e;
  aw += abs(s);
}}
vec3 look(vec2 q) {{
  vec2 at = clamp(vec2(q.x, S.y - q.y) / S, 0.001, 0.999);
  return texture2D(tex, at).rgb;
}}
void main() {{
  vec2 p = vec2(v_coords.x, 1.0 - v_coords.y) * S;
  vec2 g = vec2(0.0);
  vec2 sw = vec2(0.0);
  vec2 rs = vec2(0.0);
  float ss = 0.0;
  float ew = 0.0;
  float aw = 0.0;
{drops}  float edge = aw > 0.000001 ? ew / aw : 0.0;
  vec2 bend = g * {BEND:.1};
  vec2 fringe = rs * {BEND:.1} * {SPLIT:.3} + sw * {SWIRL:.1};
  vec3 c = vec3(look(p + bend - fringe).r, look(p + bend).g, look(p + bend + fringe).b);
  c *= 1.0 + ss * {SHADE:.2};
  vec3 light = vec3({lx:.4}, {ly:.4}, {lz:.4});
  vec3 glint = vec3(0.0);
  for (int i = 0; i < 3; i++) {{
    float m = 1.0 + (float(i) - 1.0) * {GLINT_SPLIT:.2} * edge;
    vec3 n = normalize(vec3(-g * {TILT:.1} * m, 1.0));
    glint[i] = pow(max(dot(reflect(-light, n), vec3(0.0, 0.0, 1.0)), 0.0), {SHINE:.1});
  }}
  gl_FragColor = vec4(c + glint * {GLINT:.2}, 1.0) * alpha;
}}
",
        two_width = 2.0 * WIDTH * WIDTH,
        width_sq = WIDTH * WIDTH,
        lx = light.0,
        ly = light.1,
        lz = light.2,
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
        for name in ["//_DEFINES_", "EXTERNAL", "tex", "v_coords", "alpha", "uniform vec2 S", "uniform float G"] {
            assert!(source.contains(name), "{name}");
        }
        for i in 0..DROPS {
            assert!(source.contains(&format!("uniform vec3 {};", drop_name(i))), "{i}");
            assert!(source.contains(&format!("wave(p, {}, ", drop_name(i))), "{i}");
        }
        assert_eq!(source.matches('{').count(), source.matches('}').count());
        assert!(!source.contains("NaN") && !source.contains("inf"));
    }
}
