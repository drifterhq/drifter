use std::path::{Path, PathBuf};

use crate::image::Image;

pub const EXTENSION: &str = "shader";

pub const NAMES: [&str; 8] = ["aurora", "mesh", "ink", "silk", "nebula", "topo", "caustics", "film"];

pub const HALF: [&str; 5] = ["mesh", "ink", "silk", "nebula", "film"];

pub const HEAD: &str = "#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform vec2 size;
uniform vec2 W;
uniform float alpha;
varying vec2 v_coords;
#if defined(DEBUG_FLAGS)
uniform float tint;
#endif
uniform float T;
uniform float L;
uniform vec3 S0;
uniform vec3 S1;
uniform vec3 A;
uniform vec3 B;
uniform vec3 C;
#define R W
float h(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+45.32);return fract(p.x*p.y);}
float n(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
 return mix(mix(h(i),h(i+vec2(1.,0.)),f.x),mix(h(i+vec2(0.,1.)),h(i+1.),f.x),f.y);}
float fbm(vec2 p){float v=0.,a=.5;mat2 m=mat2(1.6,1.2,-1.2,1.6);
 for(int i=0;i<5;i++){v+=a*n(p);p=m*p;a*=.5;}return v;}
float hs(vec2 p){vec3 q=fract(p.xyx*.1031);q+=dot(q,q.yzx+33.33);return fract((q.x+q.y)*q.z);}
";

pub const FOOT: &str = "
void main(){
 vec2 P=vec2(v_coords.x,1.-v_coords.y)*R;
 vec3 c=wall(P)+(h(P)-.5)/255.;
 gl_FragColor=vec4(clamp(c,0.,1.),1.)*alpha;
}
";

pub fn source(body: &str) -> String {
    format!("{HEAD}{body}{FOOT}")
}

pub fn is_shader(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn half(path: &Path) -> bool {
    HALF.contains(&stem(path).as_str())
}

pub fn folders(data_home: &Path) -> Vec<PathBuf> {
    vec![
        data_home.join("drifter/shaders"),
        PathBuf::from("/usr/local/share/drifter/shaders"),
        PathBuf::from("/usr/share/drifter/shaders"),
    ]
}

pub fn find(name: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    folders
        .iter()
        .map(|folder| folder.join(name))
        .find(|path| path.is_file())
}

pub fn installed(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        let mut here: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_shader(path) && path.is_file())
            .filter(|path| {
                !found
                    .iter()
                    .any(|seen| seen.file_name() == path.file_name())
            })
            .collect();
        here.sort();
        found.extend(here);
    }
    found
}

pub fn hour_now() -> f64 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    f64::from(now.hour())
        + f64::from(now.minute()) / 60.0
        + (f64::from(now.second()) + f64::from(now.nanosecond() % 1_000_000_000) / 1e9) / 3600.0
}

type V3 = [f64; 3];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    pub top: V3,
    pub bottom: V3,
    pub a: V3,
    pub b: V3,
    pub c: V3,
    pub light: f64,
}

const KEYS: [(f64, [u32; 5], f64); 11] = [
    (0.0, [0x04060f, 0x0a1030, 0x3dffb0, 0x7a4dff, 0x2fd4ff], 0.0),
    (4.5, [0x060a1c, 0x141a3e, 0x35e6a8, 0x8a5cff, 0x4fb8ff], 0.0),
    (5.75, [0x101845, 0x4a2f63, 0x6a7dff, 0xff8a7a, 0xffc38a], 0.15),
    (7.0, [0x3a5ea8, 0xffb08a, 0xff7a59, 0xffd27a, 0x9fb8ff], 0.55),
    (9.5, [0x4c8be0, 0xbfe0ff, 0x7fd4ff, 0xffe9a8, 0xa6f0d6], 0.88),
    (13.0, [0x3c84ea, 0xdcefff, 0x8ed8ff, 0xfff2c4, 0xb7f5e0], 1.0),
    (16.5, [0x4a76c6, 0xffd8a6, 0xffb070, 0xffe1a0, 0x8fb6ff], 0.8),
    (18.75, [0x2c3570, 0xff8a5c, 0xff5e62, 0xffb347, 0xb06ab3], 0.45),
    (20.25, [0x10163a, 0x4a2c6a, 0x7b5cff, 0xff6fa8, 0x4fd1c5], 0.15),
    (21.5, [0x060918, 0x151a40, 0x3dffb0, 0x7a4dff, 0x2fd4ff], 0.02),
    (24.0, [0x04060f, 0x0a1030, 0x3dffb0, 0x7a4dff, 0x2fd4ff], 0.0),
];

fn rgb(hex: u32) -> V3 {
    [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255].map(|v| f64::from(v) / 255.0)
}

pub fn sky(hour: f64) -> Sky {
    let hour = hour.rem_euclid(24.0);
    let at = KEYS
        .windows(2)
        .position(|pair| hour < pair[1].0)
        .unwrap_or(KEYS.len() - 2);
    let (from, to) = (KEYS[at], KEYS[at + 1]);
    let f = smoothstep(0.0, 1.0, (hour - from.0) / (to.0 - from.0));
    let blend = |i: usize| mix3(rgb(from.1[i]), rgb(to.1[i]), f);
    Sky {
        top: blend(0),
        bottom: blend(1),
        a: blend(2),
        b: blend(3),
        c: blend(4),
        light: mix(from.2, to.2, f),
    }
}

impl Sky {
    pub fn uniforms(&self) -> [(&'static str, [f32; 3]); 5] {
        let f = |v: V3| v.map(|x| x as f32);
        [
            ("S0", f(self.top)),
            ("S1", f(self.bottom)),
            ("A", f(self.a)),
            ("B", f(self.b)),
            ("C", f(self.c)),
        ]
    }
}

pub fn still(path: &Path, width: u32, height: u32, motion: f64, hour: f64) -> Image {
    let sky = sky(hour);
    let wall = cpu(&stem(path));
    let (w, h) = (width.max(1), height.max(1));
    let r = (f64::from(w), f64::from(h));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = (f64::from(x) + 0.5, f64::from(h - 1 - y) + 0.5);
            let color = wall(p, r, motion, &sky);
            for c in color {
                rgba.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
            rgba.push(255);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

type Wall = fn((f64, f64), (f64, f64), f64, &Sky) -> V3;

fn cpu(name: &str) -> Wall {
    match name {
        "aurora" => aurora,
        "mesh" => mesh,
        "ink" => ink,
        "silk" => silk,
        "nebula" => nebula,
        "topo" => topo,
        "caustics" => caustics,
        "film" => film,
        _ => plain,
    }
}

pub fn known(name: &str) -> bool {
    NAMES.contains(&name)
}

fn fract(x: f64) -> f64 {
    x - x.floor()
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn mix3(a: V3, b: V3, t: f64) -> V3 {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(x: f64, y: f64) -> f64 {
    let (mut px, mut py) = (fract(x * 123.34), fract(y * 456.21));
    let d = px * (px + 45.32) + py * (py + 45.32);
    px += d;
    py += d;
    fract(px * py)
}

fn star_hash(x: f64, y: f64) -> f64 {
    let mut q = [fract(x * 0.1031), fract(y * 0.1031), fract(x * 0.1031)];
    let d = q[0] * (q[1] + 33.33) + q[1] * (q[2] + 33.33) + q[2] * (q[0] + 33.33);
    q = q.map(|v| v + d);
    fract((q[0] + q[1]) * q[2])
}

fn noise(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    mix(
        mix(hash(ix, iy), hash(ix + 1.0, iy), ux),
        mix(hash(ix, iy + 1.0), hash(ix + 1.0, iy + 1.0), ux),
        uy,
    )
}

fn fbm(mut x: f64, mut y: f64) -> f64 {
    let (mut v, mut a) = (0.0, 0.5);
    for _ in 0..5 {
        v += a * noise(x, y);
        (x, y) = (1.6 * x - 1.2 * y, 1.2 * x + 1.6 * y);
        a *= 0.5;
    }
    v
}

fn backdrop(v: f64, sky: &Sky) -> V3 {
    mix3(sky.bottom, sky.top, v)
}

fn plain((_, py): (f64, f64), (_, ry): (f64, f64), _: f64, sky: &Sky) -> V3 {
    backdrop(py / ry, sky)
}

fn aurora((px, py): (f64, f64), (rx, ry): (f64, f64), t: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let p = (u * rx / ry, v);
    let mut col = mix3(sky.bottom, sky.top, smoothstep(0.0, 1.0, v));
    let sp = (p.0 * 180.0, p.1 * 180.0);
    let s = star_hash(sp.0.floor(), sp.1.floor());
    let (dx, dy) = (fract(sp.0) - 0.5, fract(sp.1) - 0.5);
    let size = 0.45 * (0.3 + 0.7 * fract(s * 977.0));
    let star = if s >= 0.995 { 1.0 } else { 0.0 }
        * (1.0 - smoothstep(0.0, size, (dx * dx + dy * dy).sqrt()))
        * (0.6 + 0.4 * (t * 1.5 + s * 60.0).sin());
    let lift = star * (1.0 - sky.light) * smoothstep(0.15, 0.6, v);
    col = add(col, [lift; 3]);
    let mut glow = [0.0; 3];
    for i in 0..3 {
        let fi = f64::from(i);
        let x = p.0 * (0.8 + fi * 0.35) + fi * 7.0;
        let band = 0.6 + fi * 0.08 + 0.14 * (fbm(x * 0.8, t * 0.04 + fi) - 0.5) + 0.05 * (x * 1.7 + t * 0.06).sin();
        let d = v - band;
        let curtain = (-d * d * if d < 0.0 { 260.0 } else { 22.0 }).exp();
        let rays = 0.45 + 0.55 * fbm(x * 10.0, t * 0.12 + fi * 3.0);
        let c = mix3(mix3(sky.a, sky.b, smoothstep(-0.02, 0.22, d)), sky.c, fi * 0.3);
        glow = add(glow, scale(c, curtain * rays * (1.0 - fi * 0.22)));
    }
    col = add(col, scale(glow, mix(1.15, 0.2, sky.light)));
    add(col, scale(sky.bottom, 0.18 * (-v * 5.0).exp()))
}

fn mesh((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let asp = rx / ry;
    let t = time * 0.035;
    let (x0, y0) = (u * asp, v);
    let x = x0 + 0.14 * fbm(x0 * 1.4 + t, y0 * 1.4 + t) - 0.07;
    let y = y0 + 0.14 * fbm(x0 * 1.4 - t + 5.0, y0 * 1.4 - t + 5.0) - 0.07;
    let points = [
        ((asp * (0.2 + 0.15 * (t * 1.3).sin()), 0.82 + 0.1 * (t * 1.1).cos()), sky.top),
        ((asp * (0.8 + 0.12 * (t * 0.9).cos()), 0.75 + 0.15 * (t * 1.4).sin()), sky.a),
        ((asp * (0.5 + 0.25 * (t * 0.7 + 1.0).sin()), 0.45 + 0.15 * (t * 0.8).cos()), sky.b),
        ((asp * (0.15 + 0.1 * (t * 1.2 + 2.0).cos()), 0.2 + 0.12 * t.sin()), sky.c),
        ((asp * (0.85 + 0.1 * (t * 1.1 + 3.0).sin()), 0.15 + 0.1 * (t * 1.3).cos()), sky.bottom),
    ];
    let mut sum = [0.0; 3];
    let mut total = 0.0;
    for ((qx, qy), color) in points {
        let d = (x - qx) * (x - qx) + (y - qy) * (y - qy);
        let w = 1.0 / (d + 0.02).powf(1.6);
        sum = add(sum, scale(color, w));
        total += w;
    }
    let (cx, cy) = (u - 0.5, v - 0.5);
    scale(sum, (1.0 - 0.25 * (cx * cx + cy * cy).sqrt()) / total)
}

fn ink((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let p = (u * rx / ry * 1.7, v * 1.7);
    let t = time * 0.005;
    let q = (fbm(p.0, p.1 + t), fbm(p.0 + 5.2 - t, p.1 + 1.3 - t));
    let r = (
        fbm(p.0 + 3.0 * q.0 + 1.7 + t * 0.7, p.1 + 3.0 * q.1 + 9.2 + t * 0.7),
        fbm(p.0 + 3.0 * q.0 + 8.3 - t * 0.5, p.1 + 3.0 * q.1 + 2.8 - t * 0.5),
    );
    let f = fbm(p.0 + 3.0 * r.0, p.1 + 3.0 * r.1);
    let mut col = backdrop(v, sky);
    col = mix3(col, sky.a, smoothstep(0.35, 0.85, f) * 0.8);
    col = mix3(col, sky.b, smoothstep(0.5, 1.1, (q.0 * q.0 + q.1 * q.1).sqrt()) * 0.6);
    col = mix3(col, sky.c, smoothstep(0.55, 0.9, r.0) * 0.45);
    scale(col, 0.82 + 0.35 * f)
}

fn silk((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let x = u * rx / ry;
    let t = time * 0.01;
    let mut col = backdrop(v, sky);
    for i in 0..5 {
        let fi = f64::from(i);
        let y = 0.18 + fi * 0.15 + 0.09 * (x * (1.1 + fi * 0.25) + t * (1.0 + fi * 0.2) + fi * 1.7).sin()
            + 0.08 * (fbm(x * 0.7 + fi * 3.0, t * 0.4) - 0.5);
        let w = 0.06 + 0.035 * (x * 0.8 + t * 0.7 + fi * 2.0).sin();
        let d = (v - y).abs() / w;
        let band = 1.0 - smoothstep(0.0, 1.0, d);
        let sheen = (1.0 - d).max(0.0).powi(3) * (0.6 + 0.4 * sky.light);
        let c = mix3(mix3(sky.a, sky.b, fract(fi * 0.37)), sky.c, fract(fi * 0.61));
        col = mix3(col, scale(c, 0.65 + 0.7 * sheen), band * 0.7);
    }
    col
}

fn nebula((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let asp = rx / ry;
    let p = ((u * asp - asp * 0.5) * 2.2, (v - 0.5) * 2.2);
    let t = time * 0.004;
    let w = (fbm(p.0 * 1.2 + t, p.1 * 1.2 + t), fbm(p.0 * 1.2 - t + 3.1, p.1 * 1.2 - t + 3.1));
    let d = fbm(p.0 * 1.6 + 2.2 * w.0 + t, p.1 * 1.6 + 2.2 * w.1 - t);
    let core = (-(p.0 * p.0 + p.1 * p.1) * 0.3).exp();
    let mut col = scale(backdrop(v, sky), mix(0.35, 1.0, sky.light));
    col = add(col, scale(sky.a, smoothstep(0.45, 0.9, d) * core));
    col = add(col, scale(sky.b, smoothstep(0.3, 0.75, d * w.0 * 1.6) * core * 0.8));
    col = add(col, scale(sky.c, d.powi(3) * 0.7));
    let sp = (px / ry * 260.0, py / ry * 260.0);
    let s = star_hash(sp.0.floor(), sp.1.floor());
    let (dx, dy) = (fract(sp.0) - 0.5, fract(sp.1) - 0.5);
    let star = if s >= 0.993 { 1.0 } else { 0.0 }
        * (1.0 - smoothstep(0.0, 0.45, (dx * dx + dy * dy).sqrt()))
        * (0.6 + 0.4 * (time * 1.2 + s * 50.0).sin());
    add(col, [star * (1.0 - sky.light * 0.85); 3])
}

fn height(x: f64, y: f64, t: f64) -> f64 {
    fbm(x * 0.9 + t, y * 0.9 + t * 0.6) + 0.5 * fbm(x * 2.1 - t, y * 2.1 - t)
}

fn topo((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let p = (u * rx / ry * 2.0, v * 2.0);
    let t = time * 0.006;
    let step = 2.0 / ry;
    let e = height(p.0, p.1, t);
    let ex = height(p.0 + step, p.1, t);
    let ey = height(p.0, p.1 + step, t);
    let k = e * 12.0;
    let g = ((ex - e) * (ex - e) + (ey - e) * (ey - e)).sqrt() * 12.0 + 1e-5;
    let f = fract(k);
    let d = f.min(1.0 - f) / g;
    let major = if (k + 0.5).floor().rem_euclid(5.0) < 0.5 { 1.0 } else { 0.0 };
    let lw = mix(0.7, 1.5, major);
    let line = 1.0 - smoothstep(lw, lw + 1.0, d);
    let col = mix3(backdrop(v, sky), mix3(sky.a, sky.b, smoothstep(0.3, 1.1, e)), 0.28);
    mix3(col, mix3(sky.c, [1.0; 3], 0.35), line * mix(0.45, 0.85, major))
}

fn cells(x: f64, y: f64, t: f64) -> (f64, f64) {
    let (gx, gy) = (x.floor(), y.floor());
    let (fx, fy) = (x - gx, y - gy);
    let (mut d1, mut d2) = (8.0f64, 8.0f64);
    for j in -1..=1 {
        for i in -1..=1 {
            let (ox, oy) = (f64::from(i), f64::from(j));
            let r = hash(gx + ox, gy + oy);
            let r2 = hash(gx + ox + 7.0, gy + oy + 7.0);
            let cx = ox + 0.5 + 0.4 * (t + 6.28 * r).sin() - fx;
            let cy = oy + 0.5 + 0.4 * (t * 1.3 + 6.28 * r2).cos() - fy;
            let d = cx * cx + cy * cy;
            if d < d1 {
                d2 = d1;
                d1 = d;
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    (d1.sqrt(), d2.sqrt())
}

fn caustics((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let (x0, y0) = (u * rx / ry * 3.0, v * 3.0);
    let t = time * 0.05;
    let slow = time * 0.003;
    let x = x0 + 0.3 * fbm(x0 * 0.5 + slow, y0 * 0.5 + slow);
    let y = y0 + 0.3 * fbm(x0 * 0.5 - slow + 4.0, y0 * 0.5 - slow + 4.0);
    let a = cells(x, y, t);
    let b = cells(x * 1.6 + 3.0, y * 1.6 + 3.0, t * 1.2);
    let c = (1.0 - smoothstep(0.0, 0.3, a.1 - a.0)).powi(4) * 0.7
        + (1.0 - smoothstep(0.0, 0.3, b.1 - b.0)).powi(4) * 0.45;
    let water = scale(
        mix3(mix3(sky.bottom, sky.a, 0.35), mix3(sky.top, sky.b, 0.25), v),
        mix(0.5, 1.0, sky.light),
    );
    let col = add(water, scale(mix3(sky.c, [1.0; 3], 0.6), (0.35 + 0.75 * sky.light) * c));
    let (cx, cy) = (u - 0.5, v - 0.5);
    scale(col, 1.0 - 0.3 * (cx * cx + cy * cy).sqrt())
}

fn film((px, py): (f64, f64), (rx, ry): (f64, f64), time: f64, sky: &Sky) -> V3 {
    let (u, v) = (px / rx, py / ry);
    let p = (u * rx / ry * 1.5, v * 1.5);
    let t = time * 0.006;
    let w = (fbm(p.0 + t, p.1 + t), fbm(p.0 - t + 2.7, p.1 - t + 2.7));
    let th = fbm(p.0 * 1.3 + 1.5 * w.0 + t, p.1 * 1.3 + 1.5 * w.1 - t)
        + 0.5 * fbm(p.0 * 3.0 - t * 1.5, p.1 * 3.0 - t * 1.5);
    let phase = [0.0, 0.33, 0.67];
    let irid = phase.map(|o| 0.5 + 0.5 * (6.2832 * (th * 2.2 + o)).cos());
    let base = backdrop(v, sky);
    let tint = mix3(sky.a, [1.0; 3], 0.55);
    let lit = mix(0.45, 1.0, sky.light);
    let tinted = [irid[0] * tint[0] * lit, irid[1] * tint[1] * lit, irid[2] * tint[2] * lit];
    let col = mix3(base, add(scale(base, 0.35), scale(tinted, 0.8)), 0.6);
    let glint = (th - 0.85).max(0.0).powi(2) * 4.0 * (0.3 + 0.7 * sky.light);
    add(col, scale(mix3(sky.c, [1.0; 3], 0.5), glint))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Sky, b: &Sky, by: f64) -> bool {
        let pairs = [(a.top, b.top), (a.bottom, b.bottom), (a.a, b.a), (a.b, b.b), (a.c, b.c)];
        pairs
            .iter()
            .all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() <= by))
            && (a.light - b.light).abs() <= by
    }

    #[test]
    fn the_sky_never_jumps_from_one_second_to_the_next() {
        let second = 1.0 / 3600.0;
        let mut hour = 0.0;
        while hour < 24.0 {
            assert!(close(&sky(hour), &sky(hour + second), 0.002), "jump at hour {hour}");
            hour += second * 7.0;
        }
    }

    #[test]
    fn midnight_matches_from_both_sides_and_hours_wrap() {
        assert!(close(&sky(0.0), &sky(23.9999), 0.001));
        assert_eq!(sky(25.0), sky(1.0));
        assert_eq!(sky(-1.0), sky(23.0));
    }

    #[test]
    fn noon_is_bright_and_midnight_is_dark() {
        assert!(sky(13.0).light > 0.99);
        assert!(sky(1.0).light < 0.01);
    }

    #[test]
    fn shaders_are_found_by_name_and_listed_once() {
        let root = std::env::temp_dir().join(format!("drifter-shader-dirs-{}", std::process::id()));
        let (mine, shared) = (root.join("mine"), root.join("shared"));
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(mine.join("ink.shader"), b"a").unwrap();
        std::fs::write(shared.join("ink.shader"), b"b").unwrap();
        std::fs::write(shared.join("silk.shader"), b"c").unwrap();
        std::fs::write(shared.join("note.txt"), b"d").unwrap();
        let folders = vec![mine.clone(), shared.clone()];
        assert_eq!(find("ink.shader", &folders), Some(mine.join("ink.shader")));
        assert_eq!(find("silk.shader", &folders), Some(shared.join("silk.shader")));
        assert_eq!(find("gone.shader", &folders), None);
        assert_eq!(
            installed(&folders),
            vec![mine.join("ink.shader"), shared.join("silk.shader")]
        );
    }

    #[test]
    fn every_shipped_shader_has_a_still_twin() {
        let shelf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shaders");
        let mut names: Vec<String> = std::fs::read_dir(&shelf)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_shader(path))
            .map(|path| stem(&path))
            .collect();
        names.sort();
        assert_eq!(names, ["aurora", "caustics", "film", "ink", "mesh", "nebula", "silk", "topo"]);
        for name in &names {
            assert!(known(name), "{name} has no still twin");
        }
        assert!(!known("custom"));
    }

    #[test]
    fn every_shipped_shader_reads_as_one_program() {
        let shelf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shaders");
        for entry in std::fs::read_dir(&shelf).unwrap().flatten() {
            let path = entry.path();
            if !is_shader(&path) {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(body.contains("vec3 wall(vec2 P)"), "{}", path.display());
            assert!(!body.contains("gl_FragColor"), "{}", path.display());
            assert!(!body.contains("//"), "{}", path.display());
            let full = source(&body);
            assert_eq!(full.matches('{').count(), full.matches('}').count(), "{}", path.display());
        }
    }

    #[test]
    fn a_still_has_the_asked_size_and_is_opaque() {
        for name in ["aurora", "mesh", "ink", "silk", "nebula", "topo", "caustics", "film", "other"] {
            let path = Path::new("x").join(format!("{name}.shader"));
            let image = still(&path, 24, 16, 30.0, 12.0);
            assert_eq!((image.width, image.height, image.rgba.len()), (24, 16, 24 * 16 * 4));
            assert!(image.rgba.chunks(4).all(|px| px[3] == 255), "{name}");
        }
    }

    #[test]
    fn a_night_still_is_darker_than_a_noon_still() {
        let path = Path::new("ink.shader");
        let mean = |image: &Image| image.rgba.iter().map(|&v| f64::from(v)).sum::<f64>() / image.rgba.len() as f64;
        assert!(mean(&still(path, 32, 20, 0.0, 2.0)) < mean(&still(path, 32, 20, 0.0, 13.0)));
    }

    #[test]
    fn soft_shaders_are_drawn_at_half_size() {
        assert!(half(Path::new("/a/Ink.shader")));
        assert!(!half(Path::new("/a/topo.shader")));
        assert!(!half(Path::new("/a/aurora.shader")));
    }

    #[test]
    fn stars_do_not_repeat_across_the_sky() {
        let star = |x: f64, y: f64| super::star_hash(x, y) >= 0.993;
        for period in [25.0, 50.0, 100.0] {
            let mut lit = 0;
            let mut same = 0;
            for y in 0..400 {
                for x in 0..400 {
                    let (x, y) = (f64::from(x), f64::from(y));
                    if star(x, y) {
                        lit += 1;
                        if star(x + period, y) {
                            same += 1;
                        }
                    }
                }
            }
            assert!(lit > 500, "the sky has stars");
            assert!(same * 20 < lit, "stars repeat every {period} cells: {same} of {lit}");
        }
    }

    #[test]
    fn aurora_stars_come_in_many_sizes_up_to_the_old_one() {
        let sizes: Vec<f64> = (0..4000)
            .filter_map(|i| {
                let s = super::star_hash(f64::from(i % 200), f64::from(i / 200));
                (s >= 0.995).then(|| 0.45 * (0.3 + 0.7 * super::fract(s * 977.0)))
            })
            .collect();
        assert!(sizes.len() > 5);
        let most = sizes.iter().copied().fold(0.0, f64::max);
        let least = sizes.iter().copied().fold(1.0, f64::min);
        assert!(most <= 0.45 && most > 0.35, "{most}");
        assert!(least < 0.25, "{least}");
    }
}
