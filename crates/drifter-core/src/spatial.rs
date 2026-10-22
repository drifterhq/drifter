pub const MIX: (u32, u32) = (0, 100);
pub const STEP: f64 = 0.02;
pub const FULL: u32 = 65536;

pub const QUIET_APPS: [&str; 22] = [
    "spotify",
    "com.spotify.Client",
    "rhythmbox",
    "org.gnome.Rhythmbox3",
    "lollypop",
    "org.gnome.Lollypop",
    "elisa",
    "org.kde.elisa",
    "amberol",
    "io.bassi.Amberol",
    "strawberry",
    "org.strawberrymusicplayer.strawberry",
    "audacious",
    "clementine",
    "org.clementine_player.Clementine",
    "cmus",
    "org.gnome.Music",
    "quodlibet",
    "io.github.quodlibet.QuodLibet",
    "deadbeef",
    "tauon",
    "com.github.neithern.g4music",
];

pub fn quiet_apps() -> Vec<String> {
    QUIET_APPS.iter().map(|app| app.to_string()).collect()
}

fn plain(word: &str) -> String {
    let word = word.trim().to_lowercase();
    word.strip_suffix(".desktop").map(str::to_string).unwrap_or(word)
}

pub fn ignored(list: &[String], app: &str, binary: &str, name: &str) -> bool {
    let app = plain(app);
    let tail = app.rsplit('.').next().unwrap_or(&app).to_string();
    let names = [app.clone(), tail, plain(binary), plain(name)];
    list.iter()
        .map(|each| plain(each))
        .filter(|each| !each.is_empty())
        .any(|each| names.iter().any(|name| !name.is_empty() && *name == each))
}

pub fn pan(center: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    ((center - width / 2.0) / (width / 2.0)).clamp(-1.0, 1.0)
}

pub fn gains(pan: f64, mix: f64) -> (f64, f64) {
    let p = (pan * mix.clamp(0.0, 1.0)).clamp(-1.0, 1.0);
    let angle = (p + 1.0) * std::f64::consts::FRAC_PI_4;
    let (left, right) = (angle.cos(), angle.sin());
    let most = left.max(right);
    (left / most, right / most)
}

pub fn moved(was: (f64, f64), now: (f64, f64)) -> bool {
    (was.0 - now.0).abs() > STEP || (was.1 - now.1).abs() > STEP
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub index: u32,
    pub pid: Option<u32>,
    pub binary: String,
    pub name: String,
    pub channels: Vec<String>,
    pub volume: Vec<u32>,
}

impl Stream {
    pub fn level(&self) -> u32 {
        self.volume.iter().copied().max().unwrap_or(FULL)
    }

    pub fn pans(&self) -> bool {
        self.channels.iter().any(|channel| side(channel) < 0.0) && self.channels.iter().any(|channel| side(channel) > 0.0)
    }

    pub fn volumes(&self, gains: (f64, f64)) -> Vec<u32> {
        let level = f64::from(self.level());
        self.channels
            .iter()
            .map(|channel| {
                let gain = match side(channel) {
                    s if s < 0.0 => gains.0,
                    s if s > 0.0 => gains.1,
                    _ => 1.0,
                };
                (level * gain).round() as u32
            })
            .collect()
    }

    pub fn balance(&self) -> (f64, f64) {
        let level = f64::from(self.level().max(1));
        let mean = |want: f64| {
            let picked: Vec<f64> = self
                .channels
                .iter()
                .zip(&self.volume)
                .filter(|(channel, _)| side(channel) == want)
                .map(|(_, volume)| f64::from(*volume) / level)
                .collect();
            match picked.is_empty() {
                true => 1.0,
                false => picked.iter().sum::<f64>() / picked.len() as f64,
            }
        };
        (mean(-1.0), mean(1.0))
    }
}

fn side(channel: &str) -> f64 {
    let channel = channel.to_lowercase();
    match (channel.contains("left"), channel.contains("right")) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    }
}

fn quoted(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
    Some(rest.trim_matches('"').to_string())
}

pub fn read_streams(text: &str) -> Vec<Stream> {
    let mut out: Vec<Stream> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("Sink Input #") {
            if let Ok(index) = rest.trim().parse() {
                out.push(Stream {
                    index,
                    pid: None,
                    binary: String::new(),
                    name: String::new(),
                    channels: Vec::new(),
                    volume: Vec::new(),
                });
            }
            continue;
        }
        let Some(stream) = out.last_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if let Some(map) = trimmed.strip_prefix("Channel Map:") {
            stream.channels = map.split(',').map(|each| each.trim().to_string()).filter(|each| !each.is_empty()).collect();
        } else if let Some(volume) = trimmed.strip_prefix("Volume:") {
            stream.volume = volume
                .split(',')
                .filter_map(|part| part.split(':').nth(1)?.split('/').next()?.trim().parse().ok())
                .collect();
        } else if let Some(pid) = quoted(trimmed, "application.process.id") {
            stream.pid = pid.parse().ok();
        } else if let Some(binary) = quoted(trimmed, "application.process.binary") {
            stream.binary = binary;
        } else if let Some(name) = quoted(trimmed, "application.name") {
            stream.name = name;
        }
    }
    out
}

pub fn parent(stat: &str) -> Option<u32> {
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "Sink Input #91
\tDriver: PipeWire
\tChannel Map: front-left,front-right
\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 32768 /  50% / -18.06 dB
\t        balance -0.50
\tProperties:
\t\tapplication.name = \"Firefox\"
\t\tapplication.process.id = \"4120\"
\t\tapplication.process.binary = \"firefox-esr\"

Sink Input #92
\tChannel Map: mono
\tVolume: mono: 40000 /  61% / -12.80 dB
\tProperties:
\t\tapplication.name = \"Speech\"
";

    fn near(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn a_window_in_the_middle_is_even_and_the_edges_are_full() {
        assert_eq!(pan(960.0, 1920.0), 0.0);
        assert_eq!(pan(1920.0, 1920.0), 1.0);
        assert_eq!(pan(-400.0, 1920.0), -1.0);
        assert_eq!(pan(5000.0, 1920.0), 1.0);
        assert!((pan(1440.0, 1920.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_near_side_keeps_its_level_and_the_far_side_drops() {
        assert!(near(gains(0.0, 1.0), (1.0, 1.0)));
        let (left, right) = gains(1.0, 1.0);
        assert!(left.abs() < 1e-9 && (right - 1.0).abs() < 1e-9);
        let (left, right) = gains(-0.5, 1.0);
        assert!((left - 1.0).abs() < 1e-9 && right < 0.5 && right > 0.3);
    }

    #[test]
    fn intensity_mixes_the_pan_into_plain_sound() {
        assert!(near(gains(1.0, 0.0), (1.0, 1.0)));
        let half = gains(1.0, 0.5);
        let full = gains(1.0, 1.0);
        assert!(half.0 > full.0 && half.0 < 1.0);
        assert!(near(half, gains(0.5, 1.0)));
    }

    #[test]
    fn small_changes_are_not_sent() {
        assert!(!moved((1.0, 0.5), (1.0, 0.51)));
        assert!(moved((1.0, 0.5), (1.0, 0.53)));
    }

    #[test]
    fn streams_are_read_from_pactl() {
        let streams = read_streams(LIST);
        assert_eq!(streams.len(), 2);
        let fox = &streams[0];
        assert_eq!(fox.index, 91);
        assert_eq!(fox.pid, Some(4120));
        assert_eq!(fox.binary, "firefox-esr");
        assert_eq!(fox.name, "Firefox");
        assert_eq!(fox.channels, vec!["front-left", "front-right"]);
        assert_eq!(fox.volume, vec![65536, 32768]);
        assert!(fox.pans());
        assert!(!streams[1].pans());
        assert_eq!(streams[1].pid, None);
    }

    #[test]
    fn panning_keeps_the_stream_volume() {
        let fox = &read_streams(LIST)[0];
        assert_eq!(fox.level(), 65536);
        assert_eq!(fox.balance(), (1.0, 0.5));
        assert_eq!(fox.volumes((0.5, 1.0)), vec![32768, 65536]);
        assert_eq!(fox.volumes((1.0, 1.0)), vec![65536, 65536]);
    }

    #[test]
    fn music_players_are_ignored_by_id_binary_or_name() {
        let list = quiet_apps();
        assert!(ignored(&list, "org.gnome.Rhythmbox3", "", ""));
        assert!(ignored(&list, "", "spotify", ""));
        assert!(ignored(&list, "Spotify.desktop", "", ""));
        assert!(ignored(&["Firefox".to_string()], "", "", "Firefox"));
        assert!(ignored(&["lollypop".to_string()], "org.gnome.Lollypop", "", ""));
        assert!(!ignored(&list, "firefox", "firefox", "Firefox"));
        assert!(!ignored(&[String::new()], "", "", ""));
    }

    #[test]
    fn the_parent_comes_from_proc_stat() {
        assert_eq!(parent("4121 (Web Content) S 4120 4120 4120 0 -1"), Some(4120));
        assert_eq!(parent("77 (a) b) R 12 77"), Some(12));
        assert_eq!(parent("garbage"), None);
    }
}
