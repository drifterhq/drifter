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

enum Job {
    Key(Key),
    Read,
    Set(f64),
    Light(f64),
    Mute,
}

pub struct Heard {
    pub reading: Reading,
    pub show: bool,
}

pub struct Media {
    jobs: Sender<Job>,
}

impl Media {
    pub fn press(&self, key: Key) {
        let _ = self.jobs.send(Job::Key(key));
    }

    pub fn read(&self) {
        let _ = self.jobs.send(Job::Read);
    }

    pub fn set_volume(&self, level: f64) {
        let _ = self.jobs.send(Job::Set(level.clamp(0.0, 1.0)));
    }

    pub fn set_brightness(&self, level: f64) {
        let _ = self.jobs.send(Job::Light(level.clamp(0.0, 1.0)));
    }

    pub fn toggle_mute(&self) {
        let _ = self.jobs.send(Job::Mute);
    }
}

const SINK: &str = "@DEFAULT_AUDIO_SINK@";

pub fn start() -> Result<(Media, Channel<Heard>), String> {
    let (reading_tx, reading_rx) = loop_channel::<Heard>();
    let (job_tx, job_rx) = channel();
    std::thread::Builder::new()
        .name("drifter-media".to_string())
        .spawn(move || serve(job_rx, reading_tx))
        .map_err(|e| format!("cannot start the media thread: {e}"))?;
    Ok((Media { jobs: job_tx }, reading_rx))
}

fn serve(jobs: Receiver<Job>, readings: LoopSender<Heard>) {
    let mut waiting: std::collections::VecDeque<Job> = std::collections::VecDeque::new();
    loop {
        let job = match waiting.pop_front() {
            Some(job) => job,
            None => match jobs.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
        };
        waiting.extend(jobs.try_iter());
        let (key, show) = match job {
            Job::Key(key) => (key, true),
            Job::Set(_) if matches!(waiting.front(), Some(Job::Set(_))) => continue,
            Job::Set(level) => {
                let _ = Command::new("wpctl").args(["set-mute", SINK, "0"]).status();
                let reading = volume(SINK, &["set-volume"], &format!("{level:.2}"));
                send(&readings, reading, false);
                continue;
            }
            Job::Light(_) if matches!(waiting.front(), Some(Job::Light(_))) => continue,
            Job::Light(level) => {
                send(&readings, light_at(level), false);
                continue;
            }
            Job::Mute => {
                send(&readings, volume(SINK, &["set-mute"], "toggle"), false);
                continue;
            }
            Job::Read => {
                send(&readings, read_volume(SINK), false);
                continue;
            }
        };
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
        send(&readings, reading, show);
    }
}

fn send(readings: &LoopSender<Heard>, reading: Option<Reading>, show: bool) {
    if let Some(reading) = reading {
        let _ = readings.send(Heard { reading, show });
    }
}

fn read_volume(target: &str) -> Option<Reading> {
    let out = Command::new("wpctl")
        .args(["get-volume", target])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let (level, muted) = hud::parse_wpctl(&String::from_utf8_lossy(&out.stdout))?;
    Some(Reading {
        gauge: Gauge::Volume,
        level,
        muted,
    })
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
    read_volume(target)
}

fn brightness(step: f64) -> Option<Reading> {
    let light = drifter_core::system::backlight()?;
    let value = light.step(step);
    if let Err(error) = set_brightness(&light.name, value) {
        eprintln!("drifter: cannot set the brightness: {error}");
        return None;
    }
    Some(Reading {
        gauge: Gauge::Brightness,
        level: f64::from(value) / f64::from(light.most.max(1)),
        muted: false,
    })
}

fn light_at(level: f64) -> Option<Reading> {
    let light = drifter_core::system::backlight()?;
    let value = light.raw(level);
    if let Err(error) = set_brightness(&light.name, value) {
        eprintln!("drifter: cannot set the brightness: {error}");
        return None;
    }
    Some(Reading {
        gauge: Gauge::Brightness,
        level: f64::from(value) / f64::from(light.most.max(1)),
        muted: false,
    })
}

fn set_brightness(name: &str, value: u32) -> Result<(), String> {
    let bus = zbus::blocking::Connection::system().map_err(|error| error.to_string())?;
    bus.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1/session/auto",
        Some("org.freedesktop.login1.Session"),
        "SetBrightness",
        &("backlight", name, value),
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
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
