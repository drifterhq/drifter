use crate::image::Image;
use crate::shader::Sky;

type V3 = [f64; 3];
type V2 = [f64; 2];

pub const STILL_CLOCK: f64 = 20.0;

thread_local! {
    static KEPT: std::cell::RefCell<Vec<((u32, bool, i64), Image)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn still(side: u32, hour: f64, squircle: bool) -> Image {
    let key = (side, squircle, (hour * 60.0) as i64);
    if let Some(image) = KEPT.with(|kept| kept.borrow().iter().find(|(had, _)| *had == key).map(|(_, image)| image.clone())) {
        return image;
    }
    let image = draw(side, hour, squircle);
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|((had_side, had_squircle, _), _)| (*had_side, *had_squircle) != (side, squircle));
        kept.push((key, image.clone()));
    });
    image
}

fn draw(side: u32, hour: f64, squircle: bool) -> Image {
    let sky = crate::shader::sky(hour);
    let side = side.max(1);
    let r = f64::from(side);
    let px = 2.0 / r;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let p = [f64::from(x) + 0.5, r - (f64::from(y) + 0.5)];
            let mut acc = [0.0; 4];
            for i in 0..4 {
                let o = [f64::from(i % 2) * 0.5 - 0.25, f64::from(i / 2) * 0.5 - 0.25];
                let uv = [(p[0] + o[0] - r * 0.5) / r * 2.0, (p[1] + o[1] - r * 0.5) / r * 2.0];
                let m = mark(uv, px, &sky, STILL_CLOCK, squircle);
                for k in 0..4 {
                    acc[k] += m[k];
                }
            }
            for value in acc {
                rgba.push(((value * 0.25).clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
    Image {
        width: side,
        height: side,
        rgba,
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: f64, b: f64, t: f64) -> f64 {
    a * (1.0 - t) + b * t
}

fn mix3(a: V3, b: V3, t: f64) -> V3 {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

fn times(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn plus(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn unit2(v: V2) -> V2 {
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-12);
    [v[0] / length, v[1] / length]
}

fn dot2(a: V2, b: V2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn bulge(d: f64) -> f64 {
    let k = (-d / 0.24).clamp(0.0, 1.0);
    (1.0 - (1.0 - k) * (1.0 - k)).sqrt()
}

fn lines(p: V2, sky: &Sky) -> V3 {
    let base = mix3(sky.bottom, sky.top, smoothstep(-0.9, 0.9, p[1]));
    let s = p[0] * 0.8 + p[1] * 0.6;
    let band = (s * 7.0).rem_euclid(1.0);
    let c = mix3(sky.a, sky.b, 0.5 + 0.5 * (s * 3.0).sin());
    mix3(base, c, smoothstep(0.4, 0.55, band) * smoothstep(1.0, 0.85, band) * 0.5)
}

fn field(p: V2, t: f64, squircle: bool) -> f64 {
    let an = p[1].atan2(p[0]);
    let w = 0.035 * (3.0 * an + t * 0.45).sin() + 0.025 * (5.0 * an - t * 0.33).sin() + 0.015 * (2.0 * an + t * 0.21).sin();
    match squircle {
        true => ((p[0].abs().powi(5) + p[1].abs().powi(5)).powf(0.2) - 0.66 - w * 0.08) * 0.95,
        false => ((p[0] * p[0] + p[1] * p[1]).sqrt() - 0.55 - w) * 0.95,
    }
}

fn mark(uv: V2, px: f64, sky: &Sky, t: f64, squircle: bool) -> [f64; 4] {
    let at = |dx: f64, dy: f64| field([uv[0] + dx, uv[1] + dy], t, squircle);
    let d = at(0.0, 0.0);
    let so = [0.025, -0.075];
    let sd = at(-so[0], -so[1]);
    let shadow = (1.0 - smoothstep(-0.06, 0.16, sd)) * 0.3;
    let focus = 1.0 - smoothstep(-0.18, 0.0, field([(uv[0] - so[0]) * 1.35, (uv[1] - so[1] - 0.02) * 1.35], t, squircle));
    let caustic = times(mix3(mix3(sky.a, sky.c, 0.5), [1.0; 3], 0.4), focus * 0.55);
    if d > 0.0 {
        let s = shadow * (1.0 - focus * 0.6);
        let c = times(caustic, s * 1.6);
        return [c[0], c[1], c[2], s];
    }
    let e = 0.006;
    let hx = bulge(at(e, 0.0)) - bulge(at(-e, 0.0));
    let hy = bulge(at(0.0, e)) - bulge(at(0.0, -e));
    let hgt = bulge(d);
    let raw = [-hx / (2.0 * e) * 0.16, -hy / (2.0 * e) * 0.16, 1.0];
    let length = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
    let nn = [raw[0] / length, raw[1] / length, raw[2] / length];
    let base = [uv[0] * (1.0 - 0.22 * hgt), uv[1] * (1.0 - 0.22 * hgt)];
    let off = [-nn[0] * 0.42, -nn[1] * 0.42];
    let sample = |k: f64| lines([base[0] + off[0] * k, base[1] + off[1] * k], sky);
    let mut col = [sample(1.08)[0], sample(1.0)[1], sample(0.92)[2]];
    col = plus(times(col, 1.08), [0.03; 3]);
    let ld_length = (0.25f64 + 0.49 + 0.36).sqrt();
    let ld = [-0.5 / ld_length, 0.7 / ld_length, 0.6 / ld_length];
    let toward = -(nn[0] * -ld[0] + nn[1] * -ld[1] + nn[2] * -ld[2]) * 2.0;
    let reflected_z = -ld[2] + toward * nn[2];
    let spec = reflected_z.max(0.0).powf(70.0);
    let soft = reflected_z.max(0.0).powf(8.0);
    let fres = (1.0 - nn[2]).max(0.0).powf(2.2);
    col = mix3(col, mix3(sky.top, [1.0; 3], 0.7), fres * 0.45);
    col = plus(col, [spec * 1.4 + soft * 0.12; 3]);
    let g = 0.002;
    let gd = unit2([at(g, 0.0) - at(-g, 0.0) + 1e-6, at(0.0, g) - at(0.0, -g) + 1e-6]);
    let rim = smoothstep(-0.016, -0.003, d) * smoothstep(0.0, -0.003, d);
    let lit = dot2(gd, unit2([-0.6, 0.8])).max(0.0);
    let dim = dot2(gd, unit2([0.6, -0.8])).max(0.0);
    col = plus(col, [rim * (0.25 + 0.75 * lit); 3]);
    col = plus(col, times(mix3(sky.a, sky.c, 0.5), rim * 0.5 * dim));
    col = times(col, 1.0 - 0.18 * smoothstep(-0.05, -0.01, d) * dim);
    let a = smoothstep(0.0, -px, d);
    let s = shadow * (1.0 - a);
    let out = plus(times(col, a), times(caustic, s));
    [out[0], out[1], out[2], a + s]
}
