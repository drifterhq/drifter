use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use drifter_core::blue::{self, Card, Sound};

fn read(args: &[&str]) -> Option<String> {
    Command::new("pactl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run(args: &[&str]) -> Result<(), String> {
    let out = Command::new("pactl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    match out.status.success() {
        true => Ok(()),
        false => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

pub fn card(address: &str) -> Option<Card> {
    let mut card = blue::read_card(&read(&["list", "cards"])?, address)?;
    if let Some(codec) = read(&["list", "sinks"]).and_then(|sinks| blue::sink_codec(&sinks, address)) {
        card.codec = Some(codec);
    }
    Some(card)
}

fn named(kind: &str, prefix: &str, most: Duration) -> Option<String> {
    let started = Instant::now();
    loop {
        if let Some(name) = read(&["list", "short", kind]).and_then(|short| blue::short_named(&short, prefix)) {
            return Some(name);
        }
        if started.elapsed() >= most {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn make_default(address: &str, sound: Sound) -> Result<bool, String> {
    let Some(sink) = named("sinks", &blue::sink_prefix(address), Duration::from_secs(8)) else {
        return Ok(false);
    };
    if sound != Sound::Auto
        && let Some(profile) = card(address).and_then(|card| card.profile_for(sound))
    {
        let _ = run(&["set-card-profile", &blue::card_name(address), &profile]);
    }
    let sink = named("sinks", &blue::sink_prefix(address), Duration::from_secs(3)).unwrap_or(sink);
    run(&["set-default-sink", &sink]).map(|_| true)
}

pub fn choose(address: &str, sound: Sound) -> Result<(), String> {
    let card = card(address).ok_or_else(|| drifter_core::lang::t("The headphones are not connected.").to_string())?;
    let profile = card
        .profile_for(sound)
        .ok_or_else(|| drifter_core::lang::t("These headphones offer only one choice.").to_string())?;
    run(&["set-card-profile", &card.name, &profile])?;
    if let Some(sink) = named("sinks", &blue::sink_prefix(address), Duration::from_secs(3)) {
        run(&["set-default-sink", &sink])?;
    }
    Ok(())
}

pub fn mic(address: &str, on: bool, sound: Sound) -> Result<(), String> {
    let card = card(address).ok_or_else(|| drifter_core::lang::t("The headphones are not connected.").to_string())?;
    let profile = match on {
        true => card.headset_profile().ok_or_else(|| drifter_core::lang::t("These headphones have no microphone.").to_string())?,
        false => card
            .profile_for(sound)
            .ok_or_else(|| drifter_core::lang::t("These headphones offer only one choice.").to_string())?,
    };
    run(&["set-card-profile", &card.name, &profile])?;
    if let Some(sink) = named("sinks", &blue::sink_prefix(address), Duration::from_secs(3)) {
        let _ = run(&["set-default-sink", &sink]);
    }
    if on && let Some(source) = named("sources", &blue::source_prefix(address), Duration::from_secs(3)) {
        run(&["set-default-source", &source])?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Sound,
    Mic,
}

pub fn read_event(line: &str) -> Option<Stream> {
    let rest = line.strip_prefix("Event 'new' on ")?;
    match rest.split(' ').next()? {
        "sink-input" => Some(Stream::Sound),
        "source-output" => Some(Stream::Mic),
        _ => None,
    }
}

pub fn watch_streams(tell: impl Fn(Stream) + Send + 'static) {
    let _ = std::thread::Builder::new().name("drifter-blue-streams".into()).spawn(move || {
        loop {
            let child = Command::new("pactl")
                .arg("subscribe")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(stream) = read_event(&line) {
                        tell(stream);
                    }
                }
            }
            let _ = child.wait();
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_stream_is_sound_or_mic() {
        assert_eq!(read_event("Event 'new' on sink-input #91"), Some(Stream::Sound));
        assert_eq!(read_event("Event 'new' on source-output #12"), Some(Stream::Mic));
        assert_eq!(read_event("Event 'change' on sink-input #91"), None);
        assert_eq!(read_event("Event 'new' on client #4"), None);
    }
}
