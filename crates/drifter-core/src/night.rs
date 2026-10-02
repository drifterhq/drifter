pub const PLAIN: u32 = 6500;
pub const WARMEST: u32 = 1900;

pub fn white_point(kelvin: u32) -> [f64; 3] {
    let k = f64::from(kelvin.clamp(WARMEST, PLAIN)) / 100.0;
    let red = 1.0;
    let green = (99.470_802_586_1 * k.ln() - 161.119_568_166_1) / 255.0;
    let blue = match k <= 19.0 {
        true => 0.0,
        false => (138.517_731_223_1 * (k - 10.0).ln() - 305.044_792_730_7) / 255.0,
    };
    let plain_green = (99.470_802_586_1 * 65f64.ln() - 161.119_568_166_1) / 255.0;
    let plain_blue = (138.517_731_223_1 * 55f64.ln() - 305.044_792_730_7) / 255.0;
    [
        red,
        (green / plain_green).clamp(0.0, 1.0),
        (blue / plain_blue).clamp(0.0, 1.0),
    ]
}

pub fn ramp(len: usize, kelvin: u32) -> [Vec<u16>; 3] {
    let white = white_point(kelvin);
    let channel = |scale: f64| -> Vec<u16> {
        (0..len)
            .map(|step| {
                let level = match len {
                    0 | 1 => 1.0,
                    _ => step as f64 / (len - 1) as f64,
                };
                (level * scale * 65535.0).round().clamp(0.0, 65535.0) as u16
            })
            .collect()
    };
    [channel(white[0]), channel(white[1]), channel(white[2])]
}

pub fn is_night(start: u32, end: u32, hour: u32) -> bool {
    match start.cmp(&end) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Less => (start..end).contains(&hour),
        std::cmp::Ordering::Greater => hour >= start || hour < end,
    }
}

pub fn hour_now() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_white_is_full_on_every_channel() {
        let white = white_point(PLAIN);
        for channel in white {
            assert!((channel - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn warmer_takes_away_blue_first() {
        let warm = white_point(3400);
        assert_eq!(warm[0], 1.0);
        assert!(warm[1] < 1.0 && warm[2] < warm[1]);
        assert_eq!(white_point(WARMEST)[2], 0.0);
        assert_eq!(white_point(100), white_point(WARMEST));
    }

    #[test]
    fn a_ramp_rises_from_zero_to_the_white_point() {
        let [red, green, blue] = ramp(256, 4000);
        assert_eq!((red[0], red[255]), (0, 65535));
        assert!(green[255] < 65535 && blue[255] < green[255]);
        assert!(red.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(ramp(0, 4000)[0].len(), 0);
    }

    #[test]
    fn night_can_run_through_midnight() {
        assert!(is_night(21, 7, 23) && is_night(21, 7, 0) && is_night(21, 7, 6));
        assert!(!is_night(21, 7, 7) && !is_night(21, 7, 12));
        assert!(is_night(1, 5, 3) && !is_night(1, 5, 5));
        assert!(!is_night(8, 8, 8));
    }
}
