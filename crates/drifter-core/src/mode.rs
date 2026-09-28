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
}

pub fn select(modes: &[Mode], want: Option<(u32, u32)>) -> Option<Mode> {
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
    fn the_preferred_flag_never_decides_on_its_own() {
        let picked = select(&virtio_gpu(), None).unwrap();
        assert_eq!((picked.width, picked.height), (1920, 1080));
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
    fn no_modes_means_no_choice() {
        assert_eq!(select(&[], Some((1920, 1080))), None);
    }
}
