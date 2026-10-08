#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    Line,
    Gamma(f64),
    Table(Vec<f64>),
    Param(u16, Vec<f64>),
    Range { gamma: f64, least: f64, most: f64 },
}

impl Curve {
    pub fn eval(&self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        let y = match self {
            Curve::Line => x,
            Curve::Gamma(g) => x.powf(*g),
            Curve::Range { gamma, least, most } => least + (most - least) * x.powf(*gamma),
            Curve::Table(table) => match table.len() {
                0 => x,
                1 => table[0],
                n => {
                    let at = x * (n - 1) as f64;
                    let low = (at.floor() as usize).min(n - 2);
                    let part = at - low as f64;
                    table[low] + (table[low + 1] - table[low]) * part
                }
            },
            Curve::Param(kind, p) => {
                let get = |at: usize| p.get(at).copied().unwrap_or(0.0);
                let (g, a, b, c, d, e, f) = (get(0), get(1), get(2), get(3), get(4), get(5), get(6));
                let raised = |v: f64| if v > 0.0 { v.powf(g) } else { 0.0 };
                match kind {
                    0 => raised(x),
                    1 if a != 0.0 && x >= -b / a => raised(a * x + b),
                    1 => 0.0,
                    2 if a != 0.0 && x >= -b / a => raised(a * x + b) + c,
                    2 => c,
                    3 if x >= d => raised(a * x + b),
                    3 => c * x,
                    4 if x >= d => raised(a * x + b) + e,
                    4 => c * x + f,
                    _ => x,
                }
            }
        };
        y.clamp(0.0, 1.0)
    }

    pub fn inverse(&self, y: f64) -> f64 {
        let y = y.clamp(0.0, 1.0);
        let (mut low, mut high) = (0.0, 1.0);
        let rising = self.eval(1.0) >= self.eval(0.0);
        for _ in 0..48 {
            let middle = (low + high) / 2.0;
            let below = self.eval(middle) < y;
            if below == rising {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low + high) / 2.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub trc: Option<[Curve; 3]>,
    pub colorants: Option<[[f64; 3]; 3]>,
    pub vcgt: Option<[Curve; 3]>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn fixed_at(data: &[u8], at: usize) -> Option<f64> {
    Some(i32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as f64 / 65536.0)
}

fn curve(data: &[u8]) -> Option<Curve> {
    match data.get(0..4)? {
        b"curv" => {
            let count = u32_at(data, 8)? as usize;
            match count {
                0 => Some(Curve::Line),
                1 => Some(Curve::Gamma(f64::from(u16_at(data, 12)?) / 256.0)),
                _ => (0..count)
                    .map(|at| u16_at(data, 12 + at * 2).map(|v| f64::from(v) / 65535.0))
                    .collect::<Option<Vec<f64>>>()
                    .map(Curve::Table),
            }
        }
        b"para" => {
            let kind = u16_at(data, 8)?;
            let count = [1, 3, 4, 5, 7].get(kind as usize).copied()?;
            let params = (0..count).map(|at| fixed_at(data, 12 + at * 4)).collect::<Option<Vec<f64>>>()?;
            Some(Curve::Param(kind, params))
        }
        _ => None,
    }
}

fn xyz(data: &[u8]) -> Option<[f64; 3]> {
    (data.get(0..4)? == b"XYZ ").then_some(())?;
    Some([fixed_at(data, 8)?, fixed_at(data, 12)?, fixed_at(data, 16)?])
}

fn vcgt(data: &[u8]) -> Option<[Curve; 3]> {
    (data.get(0..4)? == b"vcgt").then_some(())?;
    match u32_at(data, 8)? {
        0 => {
            let channels = u16_at(data, 12)? as usize;
            let count = u16_at(data, 14)? as usize;
            let size = u16_at(data, 16)? as usize;
            if !(size == 1 || size == 2) || count < 2 || channels == 0 {
                return None;
            }
            let read = |at: usize| -> Option<f64> {
                match size {
                    1 => data.get(18 + at).map(|v| f64::from(*v) / 255.0),
                    _ => u16_at(data, 18 + at * 2).map(|v| f64::from(v) / 65535.0),
                }
            };
            let channel = |c: usize| -> Option<Curve> {
                let c = if channels == 1 { 0 } else { c };
                (0..count).map(|at| read(c * count + at)).collect::<Option<Vec<f64>>>().map(Curve::Table)
            };
            Some([channel(0)?, channel(1)?, channel(2)?])
        }
        1 => {
            let channel = |c: usize| -> Option<Curve> {
                let at = 12 + c * 12;
                Some(Curve::Range {
                    gamma: fixed_at(data, at)?,
                    least: fixed_at(data, at + 4)?,
                    most: fixed_at(data, at + 8)?,
                })
            };
            Some([channel(0)?, channel(1)?, channel(2)?])
        }
        _ => None,
    }
}

pub fn parse(data: &[u8]) -> Result<Profile, String> {
    if data.get(36..40) != Some(b"acsp") {
        return Err(crate::lang::t("this is not an ICC color profile").to_string());
    }
    if data.get(16..20) != Some(b"RGB ") {
        return Err(crate::lang::t("only RGB display profiles can be used").to_string());
    }
    let count = u32_at(data, 128).ok_or(crate::lang::t("the profile is cut short"))? as usize;
    let mut tags = std::collections::HashMap::new();
    for at in 0..count.min(512) {
        let base = 132 + at * 12;
        let (Some(sig), Some(offset), Some(size)) = (data.get(base..base + 4), u32_at(data, base + 4), u32_at(data, base + 8)) else {
            break;
        };
        if let Some(body) = data.get(offset as usize..(offset as usize).saturating_add(size as usize)) {
            tags.insert(sig.to_vec(), body);
        }
    }
    let tag = |name: &[u8; 4]| tags.get(name.as_slice()).copied();
    let trc = match (tag(b"rTRC").and_then(curve), tag(b"gTRC").and_then(curve), tag(b"bTRC").and_then(curve)) {
        (Some(r), Some(g), Some(b)) => Some([r, g, b]),
        _ => None,
    };
    let colorants = match (tag(b"rXYZ").and_then(xyz), tag(b"gXYZ").and_then(xyz), tag(b"bXYZ").and_then(xyz)) {
        (Some(r), Some(g), Some(b)) => Some([[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]]),
        _ => None,
    };
    let vcgt = tag(b"vcgt").and_then(vcgt);
    if trc.is_none() && vcgt.is_none() {
        return Err(crate::lang::t("the profile has no tone curves Drifter can use").to_string());
    }
    Ok(Profile { trc, colorants, vcgt })
}

pub fn load(path: &std::path::Path) -> Result<Profile, String> {
    let data = std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&data)
}

pub const SRGB_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.436_074_7, 0.385_064_9, 0.143_080_4],
    [0.222_504_5, 0.716_878_6, 0.060_616_9],
    [0.013_932_2, 0.097_104_5, 0.714_173_3],
];

pub fn srgb_to_linear(x: f64) -> f64 {
    match x <= 0.040_45 {
        true => x / 12.92,
        false => ((x + 0.055) / 1.055).powf(2.4),
    }
}

fn invert(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let mut out = [[0.0; 3]; 3];
    for (row, line) in out.iter_mut().enumerate() {
        for (col, cell) in line.iter_mut().enumerate() {
            let (r1, r2) = ((col + 1) % 3, (col + 2) % 3);
            let (c1, c2) = ((row + 1) % 3, (row + 2) % 3);
            *cell = (m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1]) / det;
        }
    }
    Some(out)
}

fn times(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (row, line) in out.iter_mut().enumerate() {
        for (col, cell) in line.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[row][k] * b[k][col]).sum();
        }
    }
    out
}

pub struct Pipeline {
    pub degamma: Vec<[u16; 3]>,
    pub ctm: [[f64; 3]; 3],
    pub gamma: Vec<[u16; 3]>,
}

fn level(len: usize, at: usize) -> f64 {
    match len {
        0 | 1 => 1.0,
        _ => at as f64 / (len - 1) as f64,
    }
}

fn word(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * 65535.0).round() as u16
}

pub fn pipeline(profile: &Profile, white: [f64; 3], degamma_len: usize, gamma_len: usize) -> Option<Pipeline> {
    let trc = profile.trc.as_ref()?;
    let display = profile.colorants?;
    let ctm = times(invert(display)?, SRGB_TO_XYZ_D50);
    let degamma = (0..degamma_len)
        .map(|at| {
            let v = word(srgb_to_linear(level(degamma_len, at)));
            [v, v, v]
        })
        .collect();
    let gamma = (0..gamma_len)
        .map(|at| {
            let x = level(gamma_len, at);
            let mut out = [0u16; 3];
            for c in 0..3 {
                let encoded = trc[c].inverse(x * white[c]);
                let calibrated = profile.vcgt.as_ref().map_or(encoded, |vcgt| vcgt[c].eval(encoded));
                out[c] = word(calibrated);
            }
            out
        })
        .collect();
    Some(Pipeline { degamma, ctm, gamma })
}

pub fn calibration(profile: Option<&Profile>, white: [f64; 3], len: usize) -> [Vec<u16>; 3] {
    let channel = |c: usize| -> Vec<u16> {
        (0..len)
            .map(|at| {
                let x = level(len, at) * white[c];
                let y = profile
                    .and_then(|profile| profile.vcgt.as_ref())
                    .map_or(x, |vcgt| vcgt[c].eval(x));
                word(y)
            })
            .collect()
    };
    [channel(0), channel(1), channel(2)]
}

pub fn ctm_words(ctm: [[f64; 3]; 3]) -> [u64; 9] {
    let mut out = [0u64; 9];
    for (at, value) in ctm.iter().flatten().enumerate() {
        let magnitude = (value.abs() * 4_294_967_296.0).round() as u64 & !(1u64 << 63);
        out[at] = match *value < 0.0 {
            true => magnitude | (1u64 << 63),
            false => magnitude,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(v: f64) -> [u8; 4] {
        ((v * 65536.0).round() as i32).to_be_bytes()
    }

    fn build(tags: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0u8; 128];
        out[16..20].copy_from_slice(b"RGB ");
        out[36..40].copy_from_slice(b"acsp");
        out.extend((tags.len() as u32).to_be_bytes());
        let mut offset = 128 + 4 + tags.len() * 12;
        let mut bodies = Vec::new();
        for (sig, body) in tags {
            out.extend_from_slice(*sig);
            out.extend((offset as u32).to_be_bytes());
            out.extend((body.len() as u32).to_be_bytes());
            offset += body.len();
            bodies.extend_from_slice(body);
        }
        out.extend(bodies);
        out
    }

    fn srgb_like() -> Vec<u8> {
        let mut para = b"para\0\0\0\0".to_vec();
        para.extend(3u16.to_be_bytes());
        para.extend([0, 0]);
        for v in [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.040_45] {
            para.extend(fixed(v));
        }
        let point = |v: [f64; 3]| {
            let mut tag = b"XYZ \0\0\0\0".to_vec();
            for each in v {
                tag.extend(fixed(each));
            }
            tag
        };
        let m = SRGB_TO_XYZ_D50;
        build(&[
            (b"rTRC", para.clone()),
            (b"gTRC", para.clone()),
            (b"bTRC", para),
            (b"rXYZ", point([m[0][0], m[1][0], m[2][0]])),
            (b"gXYZ", point([m[0][1], m[1][1], m[2][1]])),
            (b"bXYZ", point([m[0][2], m[1][2], m[2][2]])),
        ])
    }

    #[test]
    fn an_srgb_display_needs_no_change() {
        let profile = parse(&srgb_like()).unwrap();
        let pipe = pipeline(&profile, [1.0; 3], 33, 1024).unwrap();
        for (row, line) in pipe.ctm.iter().enumerate() {
            for (col, cell) in line.iter().enumerate() {
                let wanted = if row == col { 1.0 } else { 0.0 };
                assert!((cell - wanted).abs() < 2e-4, "ctm {row},{col} is {cell}");
            }
        }
        for encoded in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let linear = f64::from(pipe.degamma[(encoded * 32.0) as usize][0]) / 65535.0;
            let back = pipe.gamma[(linear * 1023.0).round() as usize][0];
            let through = f64::from(back) / 65535.0;
            let wanted = level(33, (encoded * 32.0) as usize);
            assert!((through - wanted).abs() < 0.01, "{encoded}: came back as {through}");
        }
    }

    #[test]
    fn a_curve_and_its_inverse_meet() {
        for curve in [Curve::Gamma(2.2), Curve::Table(vec![0.0, 0.1, 0.5, 1.0]), Curve::Line] {
            for x in [0.0, 0.2, 0.5, 0.9, 1.0] {
                assert!((curve.eval(curve.inverse(x)) - x).abs() < 1e-6, "{curve:?} at {x}");
            }
        }
    }

    #[test]
    fn calibration_curves_shape_the_gamma_ramp() {
        let mut vcgt = b"vcgt\0\0\0\0".to_vec();
        vcgt.extend(1u32.to_be_bytes());
        for _ in 0..3 {
            vcgt.extend(fixed(1.0));
            vcgt.extend(fixed(0.1));
            vcgt.extend(fixed(0.9));
        }
        let profile = parse(&build(&[(b"vcgt", vcgt)])).unwrap();
        let ramp = calibration(Some(&profile), [1.0; 3], 3);
        for (got, wanted) in ramp[0].iter().zip([0.1, 0.5, 0.9]) {
            assert!(got.abs_diff(word(wanted)) <= 1, "{got} for {wanted}");
        }
        let warm = calibration(None, [1.0, 0.5, 0.0], 2);
        assert_eq!((warm[1][1], warm[2][1]), (word(0.5), 0));
    }

    #[test]
    fn other_files_are_refused() {
        assert!(parse(b"not a profile").is_err());
        let mut cmyk = srgb_like();
        cmyk[16..20].copy_from_slice(b"CMYK");
        assert!(parse(&cmyk).is_err());
    }

    #[test]
    fn the_matrix_is_sign_and_magnitude() {
        let words = ctm_words([[1.0, -0.5, 0.0], [0.0; 3], [0.0; 3]]);
        assert_eq!(words[0], 1u64 << 32);
        assert_eq!(words[1], (1u64 << 63) | (1u64 << 31));
    }

    #[test]
    fn the_system_srgb_profile_reads_when_present() {
        let path = std::path::Path::new("/usr/share/color/icc/ghostscript/srgb.icc");
        if path.exists() {
            let profile = load(path).unwrap();
            assert!(profile.trc.is_some() && profile.colorants.is_some());
        }
    }
}
