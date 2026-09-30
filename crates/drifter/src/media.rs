use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};

use drifter_core::hud::{self, Gauge, Reading};
use smithay::reexports::calloop::channel::{
    Channel, Sender as LoopSender, channel as loop_channel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    VolumeUp,
    VolumeDown,
    Mute,
    MicMute,
    BrightnessUp,
    BrightnessDown,
    PlayPause,
    Next,
    Previous,
    Stop,
}

pub fn key(code: u32) -> Option<Key> {
    Some(match code {
        121 => Key::Mute,
        122 => Key::VolumeDown,
        123 => Key::VolumeUp,
        171 => Key::Next,
        172 | 215 | 209 => Key::PlayPause,
        173 => Key::Previous,
        174 => Key::Stop,
        232 => Key::BrightnessDown,
        233 => Key::BrightnessUp,
        256 => Key::MicMute,
        _ => return None,
    })
}

pub struct Media {
    jobs: Sender<Key>,
}

impl Media {
    pub fn press(&self, key: Key) {
        let _ = self.jobs.send(key);
    }
}

pub fn start() -> Result<(Media, Channel<Reading>), String> {
    let (reading_tx, reading_rx) = loop_channel::<Reading>();
    let (job_tx, job_rx) = channel();
    std::thread::Builder::new()
        .name("drifter-media".to_string())
        .spawn(move || serve(job_rx, reading_tx))
        .map_err(|e| format!("cannot start the media thread: {e}"))?;
    Ok((Media { jobs: job_tx }, reading_rx))
}

fn serve(jobs: Receiver<Key>, readings: LoopSender<Reading>) {
    for key in jobs {
        let reading = match key {
            Key::VolumeUp => volume("@DEFAULT_AUDIO_SINK@", &["set-volume", "-l", "1.0"], "5%+"),
            Key::VolumeDown => volume("@DEFAULT_AUDIO_SINK@", &["set-volume"], "5%-"),
            Key::Mute => volume("@DEFAULT_AUDIO_SINK@", &["set-mute"], "toggle"),
            Key::MicMute => {
                volume("@DEFAULT_AUDIO_SOURCE@", &["set-mute"], "toggle").map(|r| Reading {
                    gauge: Gauge::Microphone,
                    ..r
                })
            }
            Key::BrightnessUp => brightness(0.05),
            Key::BrightnessDown => brightness(-0.05),
            Key::PlayPause => player("PlayPause"),
            Key::Next => player("Next"),
            Key::Previous => player("Previous"),
            Key::Stop => player("Stop"),
        };
        if let Some(reading) = reading {
            let _ = readings.send(reading);
        }
    }
}

fn volume(target: &str, verb: &[&str], amount: &str) -> Option<Reading> {
    let set = Command::new("wpctl")
        .args(verb)
        .arg(target)
        .arg(amount)
        .status();
    if let Err(error) = set {
        eprintln!("drifter: volume keys need wpctl: {error}");
        return None;
    }
    let out = Command::new("wpctl")
        .args(["get-volume", target])
        .output()
        .ok()?;
    let (level, muted) = hud::parse_wpctl(&String::from_utf8_lossy(&out.stdout))?;
    Some(Reading {
        gauge: Gauge::Volume,
        level,
        muted,
    })
}

fn backlight() -> Option<(String, u32, u32)> {
    let rank = |kind: &str| match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        _ => 2,
    };
    let mut found: Vec<(u8, String, u32, u32)> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .filter_map(|folder| {
            let read = |file: &str| std::fs::read_to_string(folder.path().join(file)).ok();
            let number = |file: &str| read(file)?.trim().parse::<u32>().ok();
            Some((
                rank(&read("type").unwrap_or_default()),
                folder.file_name().to_string_lossy().into_owned(),
                number("brightness")?,
                number("max_brightness")?,
            ))
        })
        .collect();
    found.sort();
    found
        .into_iter()
        .next()
        .map(|(_, name, now, most)| (name, now, most))
}

fn brightness(step: f64) -> Option<Reading> {
    let (name, now, most) = backlight()?;
    let most = most.max(1);
    let stride = ((f64::from(most) * step.abs()).round() as u32).max(1);
    let value = match step > 0.0 {
        true => now.saturating_add(stride).min(most),
        false => now.saturating_sub(stride).max(1),
    };
    let wanted = f64::from(value) / f64::from(most);
    let bus = zbus::blocking::Connection::system().ok()?;
    let sent = bus.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1/session/auto",
        Some("org.freedesktop.login1.Session"),
        "SetBrightness",
        &("backlight", name.as_str(), value),
    );
    if let Err(error) = sent {
        eprintln!("drifter: cannot set the brightness: {error}");
        return None;
    }
    Some(Reading {
        gauge: Gauge::Brightness,
        level: wanted,
        muted: false,
    })
}

fn player(method: &str) -> Option<Reading> {
    let address = crate::tray::bus_address().ok()?;
    let bus = zbus::blocking::connection::Builder::address(address.as_str())
        .ok()?
        .method_timeout(std::time::Duration::from_secs(1))
        .build()
        .ok()?;
    let names: Vec<String> = bus
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "ListNames",
            &(),
        )
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    let players: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
        .collect();
    let playing = |name: &str| -> bool {
        bus.call_method(
            Some(name),
            "/org/mpris/MediaPlayer2",
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.mpris.MediaPlayer2.Player", "PlaybackStatus"),
        )
        .ok()
        .and_then(|reply| {
            reply
                .body()
                .deserialize::<zbus::zvariant::OwnedValue>()
                .ok()
        })
        .and_then(|value| String::try_from(value).ok())
        .is_some_and(|status| status == "Playing")
    };
    let chosen = players
        .iter()
        .find(|name| playing(name))
        .or_else(|| players.first())?;
    if let Err(error) = bus.call_method(
        Some(chosen.as_str()),
        "/org/mpris/MediaPlayer2",
        Some("org.mpris.MediaPlayer2.Player"),
        method,
        &(),
    ) {
        eprintln!("drifter: {chosen} did not take {method}: {error}");
    }
    None
}
