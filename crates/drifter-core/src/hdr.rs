use crate::color::{Pipeline, level, srgb_to_linear, times, word};

pub const WHITE: u32 = 203;
pub const LEAST_WHITE: u32 = 80;
pub const MOST_WHITE: u32 = 500;

pub const BT709_TO_BT2020: [[f64; 3]; 3] = [
    [0.627_403_9, 0.329_283_0, 0.043_313_1],
    [0.069_097_3, 0.919_540_4, 0.011_362_3],
    [0.016_391_4, 0.088_013_3, 0.895_595_3],
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub most: Option<f64>,
    pub average: Option<f64>,
    pub least: Option<f64>,
}

pub fn read(edid: &[u8]) -> Option<Screen> {
    let count = usize::from(*edid.get(126)?);
    (1..=count).find_map(|at| {
        let block = edid.get(at * 128..(at + 1) * 128)?;
        if block[0] != 0x02 {
            return None;
        }
        let end = usize::from(block[2]).clamp(4, 127);
        let mut here = 4;
        while here < end {
            let tag = block[here] >> 5;
            let len = usize::from(block[here] & 0x1f);
            let body = block.get(here + 1..(here + 1 + len).min(end))?;
            if tag == 7 && body.len() >= 2 && body[0] == 6 {
                if body[1] & 0b100 == 0 {
                    return None;
                }
                let most = body.get(3).filter(|cv| **cv > 0).map(|cv| 50.0 * 2f64.powf(f64::from(*cv) / 32.0));
                let average = body.get(4).filter(|cv| **cv > 0).map(|cv| 50.0 * 2f64.powf(f64::from(*cv) / 32.0));
                let least = most.zip(body.get(5)).map(|(most, cv)| most * (f64::from(*cv) / 255.0).powi(2) / 100.0);
                return Some(Screen { most, average, least });
            }
            here += 1 + len;
        }
        None
    })
}

pub fn edid(name: &str) -> Option<Vec<u8>> {
    std::fs::read_dir("/sys/class/drm").ok()?.flatten().find_map(|entry| {
        let file = entry.file_name();
        let file = file.to_string_lossy();
        let (card, connector) = file.split_once('-')?;
        if !card.starts_with("card") || connector != name {
            return None;
        }
        std::fs::read(entry.path().join("edid")).ok().filter(|data| !data.is_empty())
    })
}

pub fn screen(name: &str) -> Option<Screen> {
    read(&edid(name)?)
}

pub fn pq(nits: f64) -> f64 {
    let (m1, m2) = (0.159_301_757_812_5, 78.843_75);
    let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.687_5);
    let p = (nits / 10_000.0).clamp(0.0, 1.0).powf(m1);
    ((c1 + c2 * p) / (1.0 + c3 * p)).powf(m2)
}

pub fn pipeline(white: [f64; 3], sdr: u32, degamma_len: usize, gamma_len: usize) -> Pipeline {
    let degamma = (0..degamma_len)
        .map(|at| {
            let v = word(srgb_to_linear(level(degamma_len, at)));
            [v, v, v]
        })
        .collect();
    let warm = [[white[0], 0.0, 0.0], [0.0, white[1], 0.0], [0.0, 0.0, white[2]]];
    let ctm = times(BT709_TO_BT2020, warm);
    let gamma = (0..gamma_len)
        .map(|at| {
            let v = word(pq(level(gamma_len, at) * f64::from(sdr)));
            [v, v, v]
        })
        .collect();
    Pipeline { degamma, ctm, gamma }
}

pub fn divisor(sdr: u32) -> u64 {
    (10_000.0 / f64::from(sdr.max(1)) * 4_294_967_296.0).round() as u64
}

pub fn infoframe(screen: Screen, sdr: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[4] = 2;
    let chroma = |v: f64| ((v / 0.000_02).round() as u16).to_ne_bytes();
    let points = [(0.708, 0.292), (0.170, 0.797), (0.131, 0.046), (0.312_7, 0.329_0)];
    for (at, (x, y)) in points.iter().enumerate() {
        out[6 + at * 4..8 + at * 4].copy_from_slice(&chroma(*x));
        out[8 + at * 4..10 + at * 4].copy_from_slice(&chroma(*y));
    }
    let nits = |v: f64| (v.round().clamp(0.0, 65_535.0) as u16).to_ne_bytes();
    out[22..24].copy_from_slice(&nits(screen.most.unwrap_or(1000.0)));
    out[24..26].copy_from_slice(&nits(screen.least.unwrap_or(0.0) * 10_000.0));
    out[26..28].copy_from_slice(&nits(f64::from(sdr)));
    out[28..30].copy_from_slice(&nits(f64::from(sdr)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_block(data_blocks: &[u8]) -> Vec<u8> {
        let mut edid = vec![0u8; 256];
        edid[126] = 1;
        edid[128] = 0x02;
        edid[129] = 3;
        edid[130] = (4 + data_blocks.len()) as u8;
        edid[132..132 + data_blocks.len()].copy_from_slice(data_blocks);
        edid
    }

    #[test]
    fn a_pq_screen_reads_with_its_brightness() {
        let edid = with_block(&[0x23, 0x09, 0x7f, 0x07, 0xe6, 0x06, 0x05, 0x01, 0x50, 0x40, 0x10]);
        let screen = read(&edid).unwrap();
        let most = screen.most.unwrap();
        assert!((most - 50.0 * 2f64.powf(80.0 / 32.0)).abs() < 1e-6, "{most}");
        assert!(screen.average.is_some() && screen.least.is_some());
    }

    #[test]
    fn a_screen_without_pq_is_not_hdr() {
        assert_eq!(read(&with_block(&[0xe3, 0x06, 0x01, 0x01])), None);
        assert_eq!(read(&with_block(&[0x23, 0x09, 0x7f, 0x07])), None);
        assert_eq!(read(&[0u8; 128]), None);
        assert_eq!(read(&[]), None);
    }

    #[test]
    fn a_pq_screen_without_numbers_still_reads() {
        let screen = read(&with_block(&[0xe3, 0x06, 0x05, 0x01])).unwrap();
        assert_eq!(screen, Screen { most: None, average: None, least: None });
    }

    #[test]
    fn pq_matches_the_standard() {
        assert!(pq(0.0) < 1e-6);
        assert!((pq(10_000.0) - 1.0).abs() < 1e-9);
        assert!((pq(100.0) - 0.508).abs() < 0.001);
        assert!((pq(203.0) - 0.581).abs() < 0.001);
    }

    #[test]
    fn white_lands_on_the_chosen_brightness() {
        let pipe = pipeline([1.0; 3], 203, 1024, 1024);
        assert_eq!(pipe.degamma[1023], [65535; 3]);
        let top = f64::from(pipe.gamma[1023][0]) / 65535.0;
        assert!((top - pq(203.0)).abs() < 1e-4);
        for row in pipe.ctm {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-4, "white stays white: {row:?}");
        }
        assert_eq!(divisor(10_000), 1 << 32);
        assert_eq!(divisor(200), 50 << 32);
    }

    #[test]
    fn the_infoframe_says_pq_and_bt2020() {
        let frame = infoframe(Screen { most: Some(600.0), average: None, least: Some(0.05) }, 203);
        assert_eq!(frame[..6], [0, 0, 0, 0, 2, 0]);
        assert_eq!(u16::from_ne_bytes([frame[6], frame[7]]), 35400);
        assert_eq!(u16::from_ne_bytes([frame[22], frame[23]]), 600);
        assert_eq!(u16::from_ne_bytes([frame[24], frame[25]]), 500);
        assert_eq!(u16::from_ne_bytes([frame[26], frame[27]]), 203);
    }
}
