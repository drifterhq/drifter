#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: u32,
    pub preferred: bool,
}

impl Mode {
    pub fn new(width: u32, height: u32, refresh_mhz: u32, preferred: bool) -> Self {
        Mode {
            width,
            height,
            refresh_mhz,
            preferred,
        }
    }

    pub fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub fn text(&self) -> String {
        let hertz = f64::from(self.refresh_mhz) / 1000.0;
        let rate = match self.refresh_mhz % 1000 {
            0 => format!("{}", self.refresh_mhz / 1000),
            _ => format!("{hertz:.2}"),
        };
        format!("{}x{}@{rate}", self.width, self.height)
    }

    pub fn label(&self) -> String {
        let hertz = f64::from(self.refresh_mhz) / 1000.0;
        let rate = match self.refresh_mhz % 1000 {
            0 => format!("{}", self.refresh_mhz / 1000),
            _ => format!("{hertz:.2}"),
        };
        format!("{} x {} at {rate} Hz", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wanted {
    pub width: u32,
    pub height: u32,
    pub refresh_mhz: Option<u32>,
}

pub fn parse(text: &str) -> Option<Wanted> {
    let (size, rate) = match text.trim().split_once('@') {
        Some((size, rate)) => (size, Some(rate)),
        None => (text.trim(), None),
    };
    let (width, height) = size.split_once('x')?;
    let width: u32 = width.trim().parse().ok()?;
    let height: u32 = height.trim().parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let refresh_mhz = match rate {
        None => None,
        Some(rate) => {
            let rate = rate.trim().trim_end_matches("Hz").trim();
            let hertz: f64 = rate.parse().ok()?;
            if !(1.0..=1000.0).contains(&hertz) {
                return None;
            }
            Some((hertz * 1000.0).round() as u32)
        }
    };
    Some(Wanted {
        width,
        height,
        refresh_mhz,
    })
}

pub fn pick(modes: &[Mode], wanted: Option<&str>, fallback: Option<(u32, u32)>) -> Option<Mode> {
    if let Some(wanted) = wanted.and_then(parse) {
        let sized = modes
            .iter()
            .filter(|m| m.width == wanted.width && m.height == wanted.height);
        let found = match wanted.refresh_mhz {
            Some(rate) => sized.min_by_key(|m| m.refresh_mhz.abs_diff(rate)).copied(),
            None => sized.max_by_key(|m| m.refresh_mhz).copied(),
        };
        if found.is_some() {
            return found;
        }
    }
    select(modes, fallback)
}

pub fn choices(modes: &[Mode]) -> Vec<Mode> {
    let mut out: Vec<Mode> = Vec::new();
    for mode in modes {
        let same = out.iter().any(|kept| {
            kept.width == mode.width
                && kept.height == mode.height
                && kept.refresh_mhz.abs_diff(mode.refresh_mhz) < 10
        });
        if !same {
            out.push(*mode);
        }
    }
    out.sort_by(|a, b| b.area().cmp(&a.area()).then(b.refresh_mhz.cmp(&a.refresh_mhz)));
    out
}

pub fn select(modes: &[Mode], want: Option<(u32, u32)>) -> Option<Mode> {
    let want = want.or_else(|| {
        modes
            .iter()
            .find(|m| m.preferred)
            .map(|m| (m.width, m.height))
    });
    if let Some((width, height)) = want {
        let exact = modes
            .iter()
            .filter(|m| m.width == width && m.height == height)
            .max_by_key(|m| m.refresh_mhz);
        if let Some(mode) = exact {
            return Some(*mode);
        }
    }
    modes
        .iter()
        .max_by_key(|m| (m.area(), m.refresh_mhz))
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn virtio_gpu() -> Vec<Mode> {
        vec![
            Mode::new(1280, 800, 60_000, true),
            Mode::new(1920, 1080, 60_000, false),
            Mode::new(1920, 1080, 75_000, false),
            Mode::new(1024, 768, 60_000, false),
        ]
    }

    #[test]
    fn an_exact_match_beats_the_preferred_mode() {
        let picked = select(&virtio_gpu(), Some((1920, 1080))).unwrap();
        assert_eq!((picked.width, picked.height), (1920, 1080));
        assert!(!picked.preferred);
    }

    #[test]
    fn an_exact_match_takes_the_highest_refresh() {
        let picked = select(&virtio_gpu(), Some((1920, 1080))).unwrap();
        assert_eq!(picked.refresh_mhz, 75_000);
    }

    #[test]
    fn a_monitor_gets_its_preferred_size_over_a_larger_one() {
        let modes = [
            Mode::new(4096, 2160, 60_000, false),
            Mode::new(3840, 2160, 30_000, false),
            Mode::new(3840, 2160, 60_000, true),
            Mode::new(1920, 1080, 120_000, false),
        ];
        let picked = select(&modes, None).unwrap();
        assert_eq!((picked.width, picked.height, picked.refresh_mhz), (3840, 2160, 60_000));
    }

    #[test]
    fn no_preferred_mode_falls_back_to_the_largest() {
        let modes = [Mode::new(1920, 1080, 60_000, false), Mode::new(2560, 1440, 60_000, false)];
        assert_eq!(select(&modes, None).unwrap().width, 2560);
    }

    #[test]
    fn an_unavailable_size_falls_back_to_the_largest() {
        let picked = select(&virtio_gpu(), Some((3840, 2160))).unwrap();
        assert_eq!((picked.width, picked.height), (1920, 1080));
        assert_eq!(picked.refresh_mhz, 75_000);
    }

    #[test]
    fn equal_area_is_broken_by_refresh() {
        let modes = [
            Mode::new(1600, 900, 60_000, true),
            Mode::new(1600, 900, 144_000, false),
        ];
        assert_eq!(select(&modes, None).unwrap().refresh_mhz, 144_000);
    }

    #[test]
    fn a_mode_reads_with_or_without_a_rate() {
        assert_eq!(
            parse("1920x1080@60"),
            Some(Wanted { width: 1920, height: 1080, refresh_mhz: Some(60_000) })
        );
        assert_eq!(parse("2560x1440@59.95").unwrap().refresh_mhz, Some(59_950));
        assert_eq!(parse(" 1280x800 ").unwrap().refresh_mhz, None);
        assert_eq!(parse("1280x800@144Hz").unwrap().refresh_mhz, Some(144_000));
        for bad in ["", "1920", "x1080", "0x10", "1920x1080@", "1920x1080@fast", "1920x1080@0"] {
            assert_eq!(parse(bad), None, "{bad:?} is not a mode");
        }
    }

    #[test]
    fn a_mode_writes_back_the_way_it_reads() {
        for mode in [Mode::new(1920, 1080, 60_000, false), Mode::new(2560, 1440, 59_951, false)] {
            let back = parse(&mode.text()).unwrap();
            assert_eq!((back.width, back.height), (mode.width, mode.height));
            assert!(back.refresh_mhz.unwrap().abs_diff(mode.refresh_mhz) <= 5);
        }
        assert_eq!(Mode::new(1920, 1080, 75_000, false).label(), "1920 x 1080 at 75 Hz");
    }

    #[test]
    fn the_chosen_mode_wins_and_the_closest_rate_is_taken() {
        let modes = virtio_gpu();
        let picked = pick(&modes, Some("1920x1080@60"), None).unwrap();
        assert_eq!(picked.refresh_mhz, 60_000);
        let picked = pick(&modes, Some("1920x1080@74.97"), None).unwrap();
        assert_eq!(picked.refresh_mhz, 75_000);
        let picked = pick(&modes, Some("1024x768"), None).unwrap();
        assert_eq!((picked.width, picked.height), (1024, 768));
    }

    #[test]
    fn a_chosen_mode_the_monitor_lacks_falls_back() {
        let modes = virtio_gpu();
        let picked = pick(&modes, Some("3840x2160@60"), None).unwrap();
        assert_eq!((picked.width, picked.height, picked.refresh_mhz), (1920, 1080, 75_000));
        let picked = pick(&modes, Some("nonsense"), Some((1280, 800))).unwrap();
        assert_eq!((picked.width, picked.height), (1280, 800));
    }

    #[test]
    fn the_choices_drop_repeats_and_put_the_biggest_first() {
        let mut modes = virtio_gpu();
        modes.push(Mode::new(1920, 1080, 60_002, false));
        let listed = choices(&modes);
        assert_eq!(listed.len(), 4);
        assert_eq!((listed[0].width, listed[0].refresh_mhz), (1920, 75_000));
        assert_eq!((listed[1].width, listed[1].refresh_mhz), (1920, 60_000));
        assert_eq!(listed[3].width, 1024);
    }

    #[test]
    fn no_modes_means_no_choice() {
        assert_eq!(select(&[], Some((1920, 1080))), None);
    }
}
