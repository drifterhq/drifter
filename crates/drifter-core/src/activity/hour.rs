use super::{Group, Kind, Memory};

pub const EVERY_SECONDS: u64 = 30;
pub const KEPT: usize = 120;
pub const FILE: &str = "hour";

#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub name: String,
    pub icon: Option<String>,
    pub app: bool,
    pub cpu: f64,
    pub mem_mb: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub at: u64,
    pub cpu: f64,
    pub mem: f64,
    pub swap_mb: u64,
    pub top: Vec<Share>,
}

fn clean(text: &str) -> String {
    text.replace(['|', ';', '\t', '\n'], " ")
}

impl Reading {
    pub fn take(at: u64, cpu: f64, memory: &Memory, groups: &[Group]) -> Reading {
        let mut top: Vec<&Group> = groups.iter().filter(|group| group.kind != Kind::System || group.cpu >= 1.0).collect();
        top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem_kb.cmp(&a.mem_kb)));
        let mut chosen: Vec<&Group> = top.iter().take(6).copied().collect();
        let mut heavy = top.clone();
        heavy.sort_by(|a, b| b.mem_kb.cmp(&a.mem_kb));
        for group in heavy.into_iter().filter(|group| group.mem_kb >= 200 * 1024).take(6) {
            if !chosen.iter().any(|held| held.key == group.key) {
                chosen.push(group);
            }
        }
        Reading {
            at,
            cpu,
            mem: memory.share() * 100.0,
            swap_mb: memory.swap_used_kb() / 1024,
            top: chosen
                .into_iter()
                .map(|group| Share {
                    name: group.name.clone(),
                    icon: group.icon.clone(),
                    app: group.kind == Kind::App,
                    cpu: group.cpu,
                    mem_mb: group.mem_kb / 1024,
                })
                .collect(),
        }
    }

    pub fn line(&self) -> String {
        let shares: Vec<String> = self
            .top
            .iter()
            .map(|share| format!("{};{};{:.1};{};{}", clean(&share.name), clean(share.icon.as_deref().unwrap_or("")), share.cpu, share.mem_mb, if share.app { "a" } else { "b" }))
            .collect();
        format!("{}\t{:.1}\t{:.1}\t{}\t{}", self.at, self.cpu, self.mem, self.swap_mb, shares.join("|"))
    }

    pub fn parse(line: &str) -> Option<Reading> {
        let mut parts = line.split('\t');
        let at = parts.next()?.parse().ok()?;
        let cpu = parts.next()?.parse().ok()?;
        let mem = parts.next()?.parse().ok()?;
        let swap_mb = parts.next()?.parse().ok()?;
        let top = parts
            .next()
            .unwrap_or("")
            .split('|')
            .filter(|part| !part.is_empty())
            .filter_map(|part| {
                let mut bits = part.split(';');
                let name = bits.next()?.to_string();
                let icon = bits.next().filter(|icon| !icon.is_empty()).map(str::to_string);
                let cpu = bits.next()?.parse().ok()?;
                let mem_mb = bits.next()?.parse().ok()?;
                let app = bits.next() == Some("a");
                Some(Share { name, icon, app, cpu, mem_mb })
            })
            .collect();
        Some(Reading { at, cpu, mem, swap_mb, top })
    }
}

pub fn read(text: &str, now: u64) -> Vec<Reading> {
    let oldest = now.saturating_sub(EVERY_SECONDS * KEPT as u64);
    text.lines()
        .filter_map(Reading::parse)
        .filter(|reading| reading.at >= oldest && reading.at <= now + EVERY_SECONDS)
        .collect()
}

pub fn keep(path: &std::path::Path, reading: &Reading) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let lines = std::fs::read_to_string(path).map(|text| text.lines().count()).unwrap_or(0);
    if lines >= KEPT * 2 {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let fresh = path.with_extension("new");
        std::fs::write(&fresh, append(&text, reading))?;
        return std::fs::rename(&fresh, path);
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", reading.line())
}

pub fn append(text: &str, reading: &Reading) -> String {
    let mut lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let fresh = reading.line();
    lines.push(&fresh);
    let skip = lines.len().saturating_sub(KEPT);
    let mut out = lines[skip..].join("\n");
    out.push('\n');
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Moment {
    pub at: usize,
    pub head: String,
    pub words: String,
}

fn span(points: usize) -> String {
    let seconds = points as u64 * EVERY_SECONDS;
    match seconds {
        0..60 => format!("{seconds} s"),
        _ => format!("{} min", (seconds as f64 / 60.0).round() as u64),
    }
}

pub fn moments(readings: &[Reading]) -> Vec<Moment> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < readings.len() {
        let lead = readings[at].top.first().filter(|share| share.cpu >= 40.0).cloned();
        let Some(lead) = lead else {
            at += 1;
            continue;
        };
        let mut end = at;
        let mut peak = (at, lead.cpu);
        while end + 1 < readings.len() {
            let next = readings[end + 1].top.iter().find(|share| share.name == lead.name);
            match next {
                Some(share) if share.cpu >= 25.0 => {
                    end += 1;
                    if share.cpu > peak.1 {
                        peak = (end, share.cpu);
                    }
                }
                _ => break,
            }
        }
        found.push(Moment {
            at: peak.0,
            head: format!("{} worked hard", lead.name),
            words: format!("It used up to {}% of the CPU for {}.", peak.1.round() as u64, span(end - at + 1)),
        });
        at = end + 1;
    }
    for at in 1..readings.len() {
        let (was, now) = (&readings[at - 1], &readings[at]);
        if now.at.saturating_sub(was.at) > EVERY_SECONDS * 3 {
            continue;
        }
        let earlier = &readings[at.saturating_sub(10)..at];
        for share in now.top.iter().filter(|share| share.mem_mb >= 300) {
            let seen_before = earlier.iter().any(|old| old.top.iter().any(|held| held.name == share.name));
            if !seen_before && at >= 10.min(readings.len()) && !found.iter().any(|seen: &Moment| seen.at == at) {
                found.push(Moment {
                    at,
                    head: format!("{} opened", share.name),
                    words: format!("It holds {} of memory.", super::memory_words(share.mem_mb * 1024)),
                });
            }
        }
        if now.mem >= 90.0 && was.mem < 90.0 {
            found.push(Moment {
                at,
                head: "Memory got tight".to_string(),
                words: format!("{}% of memory was in use.", now.mem.round() as u64),
            });
        }
        if now.swap_mb >= was.swap_mb + 100 {
            found.push(Moment {
                at,
                head: "Memory moved to swap".to_string(),
                words: format!("{} MB moved to the disk to make room.", now.swap_mb - was.swap_mb),
            });
        }
    }
    found.sort_by_key(|moment| moment.at);
    found
}

pub fn ago(seconds: u64) -> String {
    match seconds {
        0..45 => "Now".to_string(),
        45..90 => "1 min ago".to_string(),
        _ => format!("{} min ago", (seconds as f64 / 60.0).round() as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(at: u64, cpu: f64, top: &[(&str, f64, u64)]) -> Reading {
        Reading {
            at,
            cpu,
            mem: 50.0,
            swap_mb: 0,
            top: top
                .iter()
                .map(|(name, cpu, mem)| Share { name: name.to_string(), icon: None, app: true, cpu: *cpu, mem_mb: *mem })
                .collect(),
        }
    }

    #[test]
    fn a_reading_survives_the_file() {
        let one = Reading {
            at: 1700000000,
            cpu: 41.5,
            mem: 62.0,
            swap_mb: 12,
            top: vec![Share { name: "Fire|fox".into(), icon: Some("firefox".into()), app: true, cpu: 12.5, mem_mb: 2100 }],
        };
        let back = Reading::parse(&one.line()).unwrap();
        assert_eq!(back.top[0].name, "Fire fox");
        assert_eq!(back.top[0].icon.as_deref(), Some("firefox"));
        assert_eq!(back.cpu, 41.5);
        assert!(back.top[0].app);
    }

    #[test]
    fn the_file_keeps_only_the_last_hour() {
        let mut text = String::new();
        for at in 0..200 {
            text = append(&text, &reading(at * 30, 1.0, &[]));
        }
        assert_eq!(text.lines().count(), KEPT);
        let kept = read(&text, 199 * 30);
        assert_eq!(kept.len(), KEPT);
        assert_eq!(read(&text, 400 * 30).len(), 0, "an old file shows nothing");
    }

    #[test]
    fn a_long_spike_becomes_one_moment_at_its_peak() {
        let mut readings = vec![reading(0, 5.0, &[("Code", 3.0, 900)])];
        for (at, cpu) in [(1, 60.0), (2, 85.0), (3, 70.0)] {
            readings.push(reading(at * 30, cpu, &[("cargo", cpu, 900), ("Code", 3.0, 900)]));
        }
        readings.push(reading(120, 5.0, &[("Code", 3.0, 900)]));
        let found: Vec<Moment> = moments(&readings).into_iter().filter(|m| m.head.contains("worked")).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].at, 2);
        assert_eq!(found[0].words, "It used up to 85% of the CPU for 2 min.");
    }

    #[test]
    fn a_big_new_app_is_a_moment_but_one_that_comes_back_is_not() {
        let mut readings: Vec<Reading> = (0..12).map(|at| reading(at * 30, 5.0, &[("Code", 3.0, 900)])).collect();
        readings.push(reading(12 * 30, 9.0, &[("Blender", 4.0, 3400), ("Code", 3.0, 900)]));
        readings.push(reading(13 * 30, 9.0, &[("Code", 3.0, 900)]));
        readings.push(reading(14 * 30, 9.0, &[("Blender", 4.0, 3400), ("Code", 3.0, 900)]));
        let found: Vec<Moment> = moments(&readings).into_iter().filter(|m| m.head == "Blender opened").collect();
        assert_eq!(found.iter().map(|m| m.at).collect::<Vec<_>>(), vec![12], "{found:?}");
    }

    #[test]
    fn the_file_grows_by_appending_and_trims_itself() {
        let dir = std::env::temp_dir().join(format!("drifter-hour-{}", std::process::id()));
        let path = dir.join(FILE);
        let _ = std::fs::remove_dir_all(&dir);
        for at in 0..(KEPT as u64 * 2 + 5) {
            keep(&path, &reading(at, 1.0, &[])).unwrap();
        }
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        assert!(lines <= KEPT * 2 && lines >= KEPT, "{lines}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn minutes_ago_read_plainly() {
        assert_eq!(ago(10), "Now");
        assert_eq!(ago(60), "1 min ago");
        assert_eq!(ago(30 * 40), "20 min ago");
    }
}
