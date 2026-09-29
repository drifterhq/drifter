//! Media keys. Every action runs on its own thread, so a slow sound server
//! or player never holds up a frame. Volume goes through wpctl, brightness
//! through logind, and play and pause through the MPRIS players on the
//! session bus.

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

/// The key for an xkb keycode, which is the evdev code plus 8.
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

/// The first backlight, its brightness and its most.
fn backlight() -> Option<(String, u32, u32)> {
    let folder = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .next()?;
    let read = |file: &str| -> Option<u32> {
        std::fs::read_to_string(folder.path().join(file))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    Some((
        folder.file_name().to_string_lossy().into_owned(),
        read("brightness")?,
        read("max_brightness")?,
    ))
}

/// Changes the backlight through logind, which lets the session user do it
/// without root.
fn brightness(step: f64) -> Option<Reading> {
    let (name, now, most) = backlight()?;
    let most_f = f64::from(most.max(1));
    let wanted = (f64::from(now) / most_f + step).clamp(0.01, 1.0);
    let value = (wanted * most_f).round() as u32;
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

/// Sends a command to the player that is playing, or else to the first one.
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
