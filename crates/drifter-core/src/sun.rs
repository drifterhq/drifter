use crate::geom::Vec2;
use std::sync::Mutex;
use std::time::Instant;

const RISE: f64 = 6.0;
const SET: f64 = 19.5;
const DAY_IN: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sun {
    pub cast: Vec2,
    pub spread: f64,
    pub strength: f64,
    pub glint: [f64; 3],
    pub rim: f64,
    pub sheen: f64,
    pub side: f64,
}

fn rgb(hex: u32) -> [f64; 3] {
    [(hex >> 16) & 255, (hex >> 8) & 255, hex & 255].map(|v| f64::from(v) / 255.0)
}

fn mix3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn sun(hour: f64) -> Sun {
    let hour = hour.rem_euclid(24.0);
    let day = (hour - RISE) / (SET - RISE);
    let up = smoothstep(-0.04, 0.04, day) * (1.0 - smoothstep(0.96, 1.04, day));
    let arc = day.clamp(0.0, 1.0) * std::f64::consts::PI;
    let height = arc.sin();
    let side = -arc.cos();
    let night = (hour - SET).rem_euclid(24.0) / (24.0 - (SET - RISE));
    let moon_arc = night.clamp(0.0, 1.0) * std::f64::consts::PI;
    let moon_side = -moon_arc.cos() * moon_arc.sin() * 1.4;

    let low = 1.0 - height;
    let day_cast = Vec2::new(-side * (14.0 + 46.0 * low), 8.0 + 18.0 * low);
    let night_cast = Vec2::new(-moon_side * 16.0, 10.0);
    let cast = Vec2::new(
        night_cast.x + (day_cast.x - night_cast.x) * up,
        night_cast.y + (day_cast.y - night_cast.y) * up,
    );

    let warm = mix3(rgb(0xff9a5c), rgb(0xfff4e0), smoothstep(0.0, 0.6, height));
    let glint = mix3(rgb(0xa8bfff), warm, up);
    let rim = 0.4 + up * (0.2 + 0.35 * low);
    let sheen = 0.1 + up * (0.06 + 0.12 * low);
    Sun {
        cast,
        spread: 1.0 + 0.5 * low * up,
        strength: 1.0 + 0.4 * up * low,
        glint,
        rim,
        sheen,
        side: moon_side + (side - moon_side) * up,
    }
}

static LAPSE: Mutex<Option<(Instant, f64)>> = Mutex::new(None);

pub fn toggle_lapse(hour: f64) -> bool {
    let Ok(mut lapse) = LAPSE.lock() else {
        return false;
    };
    *lapse = match lapse.is_some() {
        true => None,
        false => Some((Instant::now(), hour)),
    };
    lapse.is_some()
}

pub fn lapsed(hour: f64) -> f64 {
    match LAPSE.lock().ok().and_then(|lapse| *lapse) {
        Some((since, from)) => {
            (from + since.elapsed().as_secs_f64() * 24.0 / DAY_IN).rem_euclid(24.0)
        }
        None => hour,
    }
}

pub fn lapsing() -> bool {
    LAPSE.lock().is_ok_and(|lapse| lapse.is_some())
}

pub fn rim_strip(sun: &Sun, length: u32, along_x: bool) -> crate::Image {
    let n = length.max(2);
    let mut rgba = Vec::with_capacity(n as usize * 4);
    let toward = match along_x {
        true => sun.side,
        false => 1.0,
    };
    for i in 0..n {
        let t = (f64::from(i) + 0.5) / f64::from(n);
        let ends = smoothstep(0.0, 0.12, t) * smoothstep(0.0, 0.12, 1.0 - t);
        let lean = match along_x {
            true => 0.55 + 0.45 * ((t - 0.5) * 2.0 * toward).clamp(-1.0, 1.0),
            false => 1.0 - smoothstep(0.0, 0.7, t),
        };
        let a = (ends * lean).clamp(0.0, 1.0);
        for c in sun.glint {
            rgba.push((c * a * 255.0).round() as u8);
        }
        rgba.push((a * 255.0).round() as u8);
    }
    let (width, height) = match along_x {
        true => (n, 1),
        false => (1, n),
    };
    crate::Image {
        width,
        height,
        rgba,
    }
}

pub fn sheen(sun: &Sun, size: u32) -> crate::Image {
    let n = size.max(2);
    let from = (0.5 + 0.6 * sun.side, -0.15);
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let u = (f64::from(x) + 0.5) / f64::from(n) - from.0;
            let v = (f64::from(y) + 0.5) / f64::from(n) - from.1;
            let (px, py) = ((f64::from(x) + 0.5) / f64::from(n), (f64::from(y) + 0.5) / f64::from(n));
            let edge = smoothstep(0.0, 0.05, px.min(1.0 - px)) * smoothstep(0.0, 0.05, py.min(1.0 - py));
            let a = ((-(u * u * 1.6 + v * v * 3.0)).exp() * edge).clamp(0.0, 1.0);
            for c in sun.glint {
                rgba.push((c * a * 255.0).round() as u8);
            }
            rgba.push((a * 255.0).round() as u8);
        }
    }
    crate::Image { width: n, height: n, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morning_shadows_fall_right_and_evening_shadows_fall_left() {
        assert!(sun(8.0).cast.x > 5.0);
        assert!(sun(17.5).cast.x < -5.0);
    }

    #[test]
    fn noon_shadows_are_short_and_straight_down() {
        let noon = sun((RISE + SET) / 2.0);
        assert!(noon.cast.x.abs() < 1.0);
        assert!(noon.cast.y < sun(7.0).cast.y);
    }

    #[test]
    fn low_sun_is_warm_and_night_is_cool() {
        let dusk = sun(19.0).glint;
        let noon = sun(12.75).glint;
        let night = sun(1.0).glint;
        assert!(dusk[0] - dusk[2] > noon[0] - noon[2]);
        assert!(night[2] > night[0]);
    }

    #[test]
    fn the_light_moves_without_jumps_through_a_whole_day() {
        let mut last = sun(0.0);
        for step in 1..=24 * 60 {
            let now = sun(f64::from(step) / 60.0);
            assert!((now.cast.x - last.cast.x).abs() < 1.5, "x jumps at {step}");
            assert!((now.cast.y - last.cast.y).abs() < 1.5, "y jumps at {step}");
            assert!((now.rim - last.rim).abs() < 0.05, "rim jumps at {step}");
            last = now;
        }
    }

    #[test]
    fn the_sheen_glows_from_the_sun_side() {
        let evening = sheen(&sun(17.5), 64);
        let alpha = |x: u32| evening.rgba[((4 * 64 + x) * 4 + 3) as usize];
        assert!(alpha(56) > alpha(8));
    }

    #[test]
    fn the_rim_is_brighter_on_the_side_toward_the_sun() {
        let morning = rim_strip(&sun(8.0), 100, true);
        let alpha = |x: usize| morning.rgba[x * 4 + 3];
        assert!(alpha(20) > alpha(80));
    }
}
