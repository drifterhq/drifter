const RATE: u32 = 48_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Shutter,
    Tick,
    Start,
    Stop,
}

impl Sound {
    pub const ALL: [Sound; 4] = [Sound::Shutter, Sound::Tick, Sound::Start, Sound::Stop];

    pub fn file_name(self) -> &'static str {
        match self {
            Sound::Shutter => "shutter.wav",
            Sound::Tick => "tick.wav",
            Sound::Start => "record-start.wav",
            Sound::Stop => "record-stop.wav",
        }
    }

    pub fn wav(self) -> Vec<u8> {
        wav(&self.samples())
    }

    fn samples(self) -> Vec<f64> {
        match self {
            Sound::Shutter => shutter(),
            Sound::Tick => tone(&[(1760.0, 0.028)], 0.22, 90.0),
            Sound::Start => tone(&[(660.0, 0.07), (990.0, 0.11)], 0.2, 30.0),
            Sound::Stop => tone(&[(990.0, 0.07), (660.0, 0.11)], 0.2, 30.0),
        }
    }
}

struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f64 / f64::from(1u32 << 31)) * 2.0 - 1.0
    }
}

fn shutter() -> Vec<f64> {
    let length = (RATE as f64 * 0.16) as usize;
    let mut out = vec![0.0; length];
    let mut noise = Noise(0x5eed);
    for (start, gain, decay, body) in [(0.0, 0.55, 140.0, 190.0), (0.072, 0.42, 110.0, 150.0)] {
        let first = (start * RATE as f64) as usize;
        let mut low = 0.0;
        for (at, sample) in out.iter_mut().enumerate().skip(first) {
            let t = (at - first) as f64 / RATE as f64;
            let fade = (-t * decay).exp();
            if fade < 0.001 {
                break;
            }
            let raw = noise.next();
            low += (raw - low) * 0.35;
            let high = raw - low;
            let click = (high * 0.7 + low * 0.3) * fade;
            let thump = (std::f64::consts::TAU * body * t).sin() * (-t * decay * 1.6).exp() * 0.6;
            *sample += (click + thump) * gain;
        }
    }
    out
}

fn tone(notes: &[(f64, f64)], gain: f64, decay: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for &(pitch, seconds) in notes {
        let length = (RATE as f64 * seconds) as usize;
        let attack = (RATE as f64 * 0.003) as usize;
        for at in 0..length {
            let t = at as f64 / RATE as f64;
            let rise = (at as f64 / attack.max(1) as f64).min(1.0);
            let tail = ((length - at) as f64 / attack.max(1) as f64).min(1.0);
            let fade = (-t * decay).exp();
            let wave = (std::f64::consts::TAU * pitch * t).sin() + (std::f64::consts::TAU * pitch * 2.0 * t).sin() * 0.18;
            out.push(wave * fade * rise * tail * gain);
        }
    }
    out
}

fn wav(samples: &[f64]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        let level = (sample.clamp(-1.0, 1.0) * f64::from(i16::MAX)).round() as i16;
        out.extend_from_slice(&level.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_a_short_clean_wav() {
        for sound in Sound::ALL {
            let bytes = sound.wav();
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..16], b"WAVEfmt ");
            let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
            assert_eq!(bytes.len(), 44 + data);
            let seconds = data as f64 / 2.0 / f64::from(RATE);
            assert!(seconds > 0.02 && seconds < 0.3, "{sound:?} lasts {seconds}");
            let loudest = bytes[44..]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
                .max()
                .unwrap();
            assert!(loudest > 2000 && loudest < 32000, "{sound:?} peaks at {loudest}");
            let last = i16::from_le_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]).unsigned_abs();
            assert!(last < 400, "{sound:?} ends with a click");
        }
    }
}
