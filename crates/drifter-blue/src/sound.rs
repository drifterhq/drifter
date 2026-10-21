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

fn has_pactl() -> bool {
    Command::new("pactl")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn needs_pactl() -> String {
    drifter_core::lang::t("Changing this needs pactl. Install pulseaudio-utils, or libpulse on Arch.").to_string()
}

fn wpctl_default(prefixes: &[String], most: Duration) -> Result<bool, String> {
    let started = Instant::now();
    loop {
        let status = Command::new("wpctl")
            .args(["status", "-n"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
        let Some(status) = status else {
            return Ok(false);
        };
        if let Some(id) = blue::wpctl_node(&status, prefixes) {
            let done = Command::new("wpctl")
                .args(["set-default", &id.to_string()])
                .stdin(Stdio::null())
                .status()
                .map_err(|error| error.to_string())?;
            return Ok(done.success());
        }
        if started.elapsed() >= most {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn named(kind: &str, prefixes: &[String], most: Duration) -> Option<String> {
    let started = Instant::now();
    loop {
        if let Some(name) = read(&["list", "short", kind]).and_then(|short| blue::short_named(&short, prefixes)) {
            return Some(name);
        }
        if started.elapsed() >= most {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

pub fn make_default(address: &str, sound: Sound) -> Result<bool, String> {
    if !has_pactl() {
        return wpctl_default(&blue::sink_prefixes(address), Duration::from_secs(8));
    }
    let Some(sink) = named("sinks", &blue::sink_prefixes(address), Duration::from_secs(8)) else {
        return Ok(false);
    };
    if sound != Sound::Auto
        && let Some(profile) = card(address).and_then(|card| card.profile_for(sound))
    {
        let _ = run(&["set-card-profile", &blue::card_name(address), &profile]);
    }
    let sink = named("sinks", &blue::sink_prefixes(address), Duration::from_secs(3)).unwrap_or(sink);
    run(&["set-default-sink", &sink]).map(|_| true)
}

pub fn choose(address: &str, sound: Sound) -> Result<(), String> {
    if !has_pactl() {
        return Err(needs_pactl());
    }
    let card = card(address).ok_or_else(|| drifter_core::lang::t("The headphones are not connected.").to_string())?;
    let profile = card
        .profile_for(sound)
        .ok_or_else(|| drifter_core::lang::t("These headphones offer only one choice.").to_string())?;
    run(&["set-card-profile", &card.name, &profile])?;
    if let Some(sink) = named("sinks", &blue::sink_prefixes(address), Duration::from_secs(3)) {
        run(&["set-default-sink", &sink])?;
    }
    Ok(())
}

pub fn mic(address: &str, on: bool, sound: Sound) -> Result<(), String> {
    if !has_pactl() {
        return Err(needs_pactl());
    }
    let card = card(address).ok_or_else(|| drifter_core::lang::t("The headphones are not connected.").to_string())?;
    let profile = match on {
        true => card.headset_profile().ok_or_else(|| drifter_core::lang::t("These headphones have no microphone.").to_string())?,
        false => card
            .profile_for(sound)
            .ok_or_else(|| drifter_core::lang::t("These headphones offer only one choice.").to_string())?,
    };
    run(&["set-card-profile", &card.name, &profile])?;
    if let Some(sink) = named("sinks", &blue::sink_prefixes(address), Duration::from_secs(3)) {
        let _ = run(&["set-default-sink", &sink]);
    }
    if on && let Some(source) = named("sources", &blue::source_prefixes(address), Duration::from_secs(3)) {
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

    fn fake(dir: &std::path::Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn the_sound_commands_reach_pactl_and_wpctl_with_the_right_words() {
        let dir = std::env::temp_dir().join(format!("drifter-fake-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log");
        let cards = "Card #52\n\tName: bluez_card.AA_BB_CC_DD_EE_FF\n\tProfiles:\n\t\ta2dp-sink-sbc: x (sinks: 1, sources: 0, priority: 18, available: yes)\n\t\ta2dp-sink-ldac: x (sinks: 1, sources: 0, priority: 20, available: yes)\n\t\ta2dp-sink: x (sinks: 1, sources: 0, priority: 21, available: yes)\n\t\theadset-head-unit: x (sinks: 1, sources: 1, priority: 2, available: yes)\n\tActive Profile: a2dp-sink\n";
        std::fs::write(dir.join("cards"), cards).unwrap();
        std::fs::write(dir.join("sinks"), "77\tbluez_output.AA_BB_CC_DD_EE_FF.1\tPipeWire\ts16le\tRUNNING\n").unwrap();
        std::fs::write(dir.join("sources"), "78\tbluez_input.AA_BB_CC_DD_EE_FF.0\tPipeWire\ts16le\tRUNNING\n").unwrap();
        let d = dir.display();
        fake(&dir, "pactl", &format!(
            "echo \"$@\" >> {d}/log\ncase \"$1 $2\" in\n\"--version \") exit 0;;\n\"list cards\") /bin/cat {d}/cards;;\n\"list sinks\") exit 0;;\n\"list short\") /bin/cat {d}/$3;;\nesac\nexit 0"
        ));
        let was = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", &dir) };
        let switched = make_default("AA:BB:CC:DD:EE:FF", Sound::Best);
        let mic_on = mic("AA:BB:CC:DD:EE:FF", true, Sound::Best);
        let said = std::fs::read_to_string(&log).unwrap_or_default();
        std::fs::remove_file(dir.join("pactl")).unwrap();
        std::fs::write(&log, "").unwrap();
        fake(&dir, "wpctl", &format!(
            "echo \"$@\" >> {d}/log\nif [ \"$1\" = status ]; then echo ' │      77. bluez_output.AA_BB_CC_DD_EE_FF.1 [vol: 0.80]'; fi\nexit 0"
        ));
        let fallback = make_default("AA:BB:CC:DD:EE:FF", Sound::Auto);
        let quality = choose("AA:BB:CC:DD:EE:FF", Sound::Best);
        let said_wpctl = std::fs::read_to_string(&log).unwrap_or_default();
        match was {
            Some(path) => unsafe { std::env::set_var("PATH", path) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(switched, Ok(true), "{said}");
        assert!(said.contains("set-card-profile bluez_card.AA_BB_CC_DD_EE_FF a2dp-sink-ldac"), "{said}");
        assert!(said.contains("set-default-sink bluez_output.AA_BB_CC_DD_EE_FF.1"), "{said}");
        assert_eq!(mic_on, Ok(()));
        assert!(said.contains("set-card-profile bluez_card.AA_BB_CC_DD_EE_FF headset-head-unit"), "{said}");
        assert!(said.contains("set-default-source bluez_input.AA_BB_CC_DD_EE_FF.0"), "{said}");
        assert_eq!(fallback, Ok(true), "{said_wpctl}");
        assert!(said_wpctl.contains("set-default 77"), "without pactl, wpctl picks the headphones: {said_wpctl}");
        assert!(quality.is_err_and(|error| error.contains("pactl")), "without pactl the reason is plain");
    }
}
