use std::path::Path;

use crate::sheet::Choice;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sound {
    pub ready: bool,
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub volume: f64,
    pub muted: bool,
    pub mic: f64,
    pub mic_muted: bool,
}

impl Sound {
    pub fn output(&self) -> Option<&Device> {
        self.outputs.iter().find(|device| device.default)
    }

    pub fn input(&self) -> Option<&Device> {
        self.inputs.iter().find(|device| device.default)
    }
}

pub fn choices(devices: &[Device]) -> Vec<Choice> {
    devices
        .iter()
        .map(|device| Choice::new(&device.id.to_string(), &device.name))
        .collect()
}

fn device(line: &str) -> Option<Device> {
    let line = line.trim_start_matches([' ', '│', '├', '└', '─']);
    let (default, line) = match line.strip_prefix('*') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, line),
    };
    let (id, rest) = line.split_once(". ")?;
    let id = id.trim().parse().ok()?;
    let name = match rest.rfind(" [") {
        Some(at) => &rest[..at],
        None => rest,
    };
    let name = name.trim();
    (!name.is_empty()).then(|| Device {
        id,
        name: name.to_string(),
        default,
    })
}

pub fn parse_status(text: &str) -> (Vec<Device>, Vec<Device>) {
    let (mut outputs, mut inputs) = (Vec::new(), Vec::new());
    let mut audio = false;
    let mut list: Option<bool> = None;
    for line in text.lines() {
        if !line.starts_with([' ', '│', '├', '└']) && !line.trim().is_empty() {
            audio = line.trim() == "Audio";
            list = None;
            continue;
        }
        if !audio {
            continue;
        }
        let bare = line.trim_start_matches([' ', '│']);
        if let Some(head) = bare
            .strip_prefix("├─ ")
            .or_else(|| bare.strip_prefix("└─ "))
        {
            list = match head.trim() {
                "Sinks:" => Some(true),
                "Sources:" => Some(false),
                _ => None,
            };
            continue;
        }
        match (list, device(line)) {
            (Some(true), Some(found)) => outputs.push(found),
            (Some(false), Some(found)) => inputs.push(found),
            _ => {}
        }
    }
    (outputs, inputs)
}

pub fn zone_choices(table: &str) -> Vec<Choice> {
    let mut zones: Vec<&str> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split('\t').nth(2))
        .map(str::trim)
        .filter(|zone| !zone.is_empty())
        .collect();
    zones.push("UTC");
    zones.sort_unstable();
    zones.dedup();
    zones
        .into_iter()
        .map(|zone| Choice::new(zone, &zone.replace('_', " ")))
        .collect()
}

pub fn zone_from_link(target: &Path) -> Option<String> {
    let text = target.to_string_lossy();
    let (_, zone) = text.split_once("zoneinfo/")?;
    (!zone.is_empty()).then(|| zone.to_string())
}

pub fn os_name(release: &str) -> Option<String> {
    let value = |key: &str| {
        release.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().trim_matches('"').to_string())
        })
    };
    value("PRETTY_NAME").or_else(|| value("NAME"))
}

pub fn cpu_name(info: &str) -> Option<String> {
    let model = info.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        (name.trim() == "model name").then(|| value.trim().to_string())
    })?;
    let cores = info
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count();
    Some(match cores {
        0 | 1 => model,
        cores => format!("{model}, {cores} threads"),
    })
}

pub fn memory(info: &str) -> Option<String> {
    let kib: f64 = info.lines().find_map(|line| {
        let rest = line.strip_prefix("MemTotal:")?;
        rest.trim().trim_end_matches("kB").trim().parse().ok()
    })?;
    Some(format!("{:.1} GB", kib / 1024.0 / 1024.0))
}

pub fn about(renderer: Option<&str>, config: &Path) -> Vec<(String, String)> {
    let read = |path: &str| std::fs::read_to_string(path).ok();
    let mut rows = vec![(
        "Drifter".to_string(),
        format!("Version {}", env!("CARGO_PKG_VERSION")),
    )];
    let mut add = |key: &str, value: Option<String>| {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            rows.push((key.to_string(), value.trim().to_string()));
        }
    };
    add("Computer", read("/proc/sys/kernel/hostname"));
    add(
        "System",
        read("/etc/os-release").and_then(|text| os_name(&text)),
    );
    add("Kernel", read("/proc/sys/kernel/osrelease"));
    add(
        "Processor",
        read("/proc/cpuinfo").and_then(|text| cpu_name(&text)),
    );
    add(
        "Memory",
        read("/proc/meminfo").and_then(|text| memory(&text)),
    );
    add("Graphics", renderer.map(str::to_string));
    add("Config file", Some(config.display().to_string()));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "PipeWire 'pipewire-0' [1.2.7, me@box, cookie:1]
 └─ Clients:
        33. WirePlumber                         [1.2.7, me@box, pid:900]

Audio
 ├─ Devices:
 │      42. Built-in Audio                      [alsa]
 │
 ├─ Sinks:
 │      46. Built-in Audio Analog Stereo        [vol: 0.40]
 │  *   51. HDMI Output                         [vol: 1.00 MUTED]
 │
 ├─ Sink endpoints:
 │
 ├─ Sources:
 │  *   47. Built-in Audio Analog Stereo        [vol: 1.00]
 │
 ├─ Source endpoints:
 │
 └─ Streams:
        60. Firefox

Video
 ├─ Devices:
 │      70. Camera                              [v4l2]
 ├─ Sinks:
 │  *   71. Not audio
";

    #[test]
    fn sinks_and_sources_come_from_the_audio_part_only() {
        let (outputs, inputs) = parse_status(STATUS);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].name, "Built-in Audio Analog Stereo");
        assert!(!outputs[0].default && outputs[1].default);
        assert_eq!(outputs[1].id, 51);
        assert_eq!(inputs.len(), 1);
        assert!(inputs[0].default);
    }

    #[test]
    fn zones_are_read_from_the_table_with_utc_added() {
        let table = "# comment\nUS\t+404251-0740023\tAmerica/New_York\tEastern\nGB\t+513030-0000731\tEurope/London\n";
        let zones = zone_choices(table);
        let codes: Vec<&str> = zones.iter().map(|zone| zone.code.as_str()).collect();
        assert_eq!(codes, ["America/New_York", "Europe/London", "UTC"]);
        assert_eq!(zones[0].name, "America/New York");
    }

    #[test]
    fn the_zone_is_read_from_the_localtime_link() {
        let link = Path::new("/usr/share/zoneinfo/America/Chicago");
        assert_eq!(zone_from_link(link).as_deref(), Some("America/Chicago"));
        assert_eq!(zone_from_link(Path::new("/etc/other")), None);
    }

    #[test]
    fn system_files_read_well() {
        assert_eq!(
            os_name("NAME=\"Debian\"\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n").as_deref(),
            Some("Debian GNU/Linux 13 (trixie)")
        );
        let cpu = "processor\t: 0\nmodel name\t: AMD Ryzen 7\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7\n";
        assert_eq!(cpu_name(cpu).as_deref(), Some("AMD Ryzen 7, 2 threads"));
        assert_eq!(
            memory("MemTotal:       16303112 kB\nMemFree: 1 kB\n").as_deref(),
            Some("15.5 GB")
        );
    }
}
