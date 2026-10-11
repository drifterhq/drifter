pub mod glance;
pub mod hour;
pub mod look;

use std::collections::HashMap;

use crate::desktop::App;

pub const ACTIVITY_APP: &str = "drifter-activity";
pub const SETTINGS_APP: &str = "drifter-settings";

pub fn art_icon() -> String {
    if crate::icon::find(ACTIVITY_APP, None, 64).is_some() {
        return ACTIVITY_APP.to_string();
    }
    static FOUND: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FOUND
        .get_or_init(|| {
            crate::Config::load_from(&crate::Config::config_path())
                .ok()
                .and_then(|config| config.dock_dir())
                .map(|dir| dir.join("activity.png"))
                .filter(|path| path.is_file())
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_else(|| "utilities-system-monitor".to_string())
        })
        .clone()
}

pub fn own_app() -> Option<App> {
    let program = std::env::current_exe().ok()?.with_file_name(ACTIVITY_APP);
    program.is_file().then(|| App {
        id: ACTIVITY_APP.to_string(),
        name: crate::lang::t("Activity").to_string(),
        icon: Some(art_icon()),
        exec: Some(format!("\"{}\"", program.display())),
        words: "task manager process cpu memory monitor system".to_string(),
        categories: vec!["System".to_string(), "Monitor".to_string()],
        terminal: false,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub comm: String,
    pub state: char,
    pub ticks: u64,
    pub start: u64,
    pub rss_kb: u64,
    pub uid: Option<u32>,
    pub argv: Vec<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Memory {
    pub total_kb: u64,
    pub available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

impl Memory {
    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.available_kb)
    }

    pub fn swap_used_kb(&self) -> u64 {
        self.swap_total_kb.saturating_sub(self.swap_free_kb)
    }

    pub fn share(&self) -> f64 {
        match self.total_kb {
            0 => 0.0,
            total => self.used_kb() as f64 / total as f64,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub procs: Vec<Proc>,
    pub total_ticks: u64,
    pub busy_ticks: u64,
    pub memory: Memory,
    pub cores: usize,
}

pub fn parse_stat(text: &str) -> Option<(u32, String, char, u32, u64)> {
    parse_stat_full(text).map(|(pid, comm, state, ppid, ticks, _)| (pid, comm, state, ppid, ticks))
}

pub fn parse_stat_full(text: &str) -> Option<(u32, String, char, u32, u64, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let pid = text[..open].trim().parse().ok()?;
    let comm = text[open + 1..close].to_string();
    let rest: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let state = rest.first()?.chars().next()?;
    let ppid = rest.get(1)?.parse().ok()?;
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let start: u64 = rest.get(19).and_then(|word| word.parse().ok()).unwrap_or(0);
    Some((pid, comm, state, ppid, utime + stime, start))
}

pub fn parse_statm(text: &str, page_kb: u64) -> Option<u64> {
    let resident: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident * page_kb)
}

pub fn parse_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

pub fn parse_argv(cmdline: &[u8]) -> Vec<String> {
    cmdline
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

pub fn unit_of(cgroup: &str) -> Option<String> {
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    let last = path.trim().rsplit('/').next()?;
    let unit = last
        .strip_suffix(".service")
        .or_else(|| last.strip_suffix(".scope"))?;
    let unit = unit.split('@').next().unwrap_or(unit);
    match unit.is_empty() || unit.starts_with("session-") || unit == "init" {
        true => None,
        false => Some(unit.to_string()),
    }
}

pub fn parse_memory(meminfo: &str) -> Memory {
    let field = |name: &str| {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Memory {
        total_kb: field("MemTotal:"),
        available_kb: field("MemAvailable:"),
        swap_total_kb: field("SwapTotal:"),
        swap_free_kb: field("SwapFree:"),
    }
}

pub fn physical_cores(cpuinfo: &str) -> Option<usize> {
    let mut seen = std::collections::HashSet::new();
    let (mut socket, mut core) = (None, None);
    for line in cpuinfo.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let Some(core) = core.take() {
                seen.insert((socket.take().unwrap_or(0u32), core));
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "physical id" => socket = value.trim().parse().ok(),
            "core id" => core = value.trim().parse::<u32>().ok(),
            _ => {}
        }
    }
    (!seen.is_empty()).then_some(seen.len())
}

pub fn read_snapshot() -> Snapshot {
    let stat = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let times = crate::stats::cpu_times(&stat).unwrap_or(crate::stats::CpuTimes { busy: 0, total: 0 });
    let threads = stat
        .lines()
        .filter(|line| line.starts_with("cpu") && line.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .count()
        .max(1);
    let cores = std::fs::read_to_string("/proc/cpuinfo").ok().as_deref().and_then(physical_cores).unwrap_or(threads);
    let memory = parse_memory(&std::fs::read_to_string("/proc/meminfo").unwrap_or_default());
    let mut procs = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
                continue;
            };
            if let Some(proc) = read_proc(pid) {
                procs.push(proc);
            }
        }
    }
    Snapshot {
        procs,
        total_ticks: times.total,
        busy_ticks: times.busy,
        memory,
        cores,
    }
}

fn read_proc(pid: u32) -> Option<Proc> {
    let dir = std::path::PathBuf::from(format!("/proc/{pid}"));
    let (pid, comm, state, ppid, ticks, start) = parse_stat_full(&std::fs::read_to_string(dir.join("stat")).ok()?)?;
    if pid == 2 || ppid == 2 {
        return None;
    }
    let rss_kb = std::fs::read_to_string(dir.join("statm"))
        .ok()
        .and_then(|text| parse_statm(&text, 4))
        .unwrap_or(0);
    let uid = std::fs::read_to_string(dir.join("status")).ok().as_deref().and_then(parse_uid);
    let argv = std::fs::read(dir.join("cmdline")).map(|bytes| parse_argv(&bytes)).unwrap_or_default();
    let unit = std::fs::read_to_string(dir.join("cgroup")).ok().as_deref().and_then(unit_of);
    Some(Proc {
        pid,
        ppid,
        comm,
        state,
        ticks,
        start,
        rss_kb,
        uid,
        argv,
        unit,
    })
}

pub fn own_uid() -> Option<u32> {
    parse_uid(&std::fs::read_to_string("/proc/self/status").ok()?)
}

pub fn find_shell(snapshot: &Snapshot, me: u32) -> Option<u32> {
    let mine = snapshot.procs.iter().find(|proc| proc.pid == me);
    let parent = mine.and_then(|proc| snapshot.procs.iter().find(|up| up.pid == proc.ppid));
    if let Some(parent) = parent.filter(|up| up.comm == "drifter") {
        return Some(parent.pid);
    }
    let uid = mine.and_then(|proc| proc.uid);
    snapshot
        .procs
        .iter()
        .find(|proc| proc.comm == "drifter" && (uid.is_none() || proc.uid == uid))
        .map(|proc| proc.pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Pause,
    Resume,
    Quit,
    Force,
}


impl Signal {
    fn number(self) -> i32 {
        match self {
            Signal::Pause => libc::SIGSTOP,
            Signal::Resume => libc::SIGCONT,
            Signal::Quit => libc::SIGTERM,
            Signal::Force => libc::SIGKILL,
        }
    }

    pub fn failed(self, name: &str) -> String {
        match self {
            Signal::Pause => crate::lang::tf("Could not pause {}.", &[&(name)]),
            Signal::Resume => crate::lang::tf("Could not resume {}.", &[&(name)]),
            Signal::Quit => crate::lang::tf("Could not quit {}.", &[&(name)]),
            Signal::Force => crate::lang::tf("Could not force quit {}.", &[&(name)]),
        }
    }
}

fn still_same(pid: u32, start: u64) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|text| parse_stat_full(&text))
        .is_some_and(|(_, _, _, _, _, now)| start == 0 || now == start)
}

pub fn send(signal: Signal, group: &Group) -> Result<(), String> {
    let targets: Vec<(u32, u64)> = match signal {
        Signal::Quit => group.members.iter().filter(|member| member.pid == group.root).map(|member| (member.pid, member.start)).collect(),
        _ => group.members.iter().map(|member| (member.pid, member.start)).collect(),
    };
    let targets = match targets.is_empty() {
        true => vec![(group.root, 0)],
        false => targets,
    };
    send_checked(signal, group.root, &targets)
}

pub fn send_checked(signal: Signal, root: u32, targets: &[(u32, u64)]) -> Result<(), String> {
    let mut root_error = None;
    let mut any = false;
    for (pid, start) in targets {
        if *pid <= 1 || !still_same(*pid, *start) {
            continue;
        }
        let done = unsafe { libc::kill(*pid as libc::pid_t, signal.number()) } == 0;
        let error = std::io::Error::last_os_error();
        match done {
            true => any = true,
            false if error.raw_os_error() == Some(libc::ESRCH) => {}
            false if *pid == root => root_error = Some(error.to_string()),
            false => {}
        }
    }
    match (root_error, any) {
        (Some(error), false) => Err(error),
        _ => Ok(()),
    }
}

pub fn send_to(signal: Signal, pids: &[String]) -> Result<(), String> {
    let targets: Vec<(u32, u64)> = pids.iter().filter_map(|pid| pid.parse().ok()).map(|pid| (pid, 0)).collect();
    let root = targets.first().map_or(0, |(pid, _)| *pid);
    send_checked(signal, root, &targets)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gone {
    pub name: String,
    pub argv: Vec<String>,
    pub cwd: Option<String>,
}

impl Gone {
    pub fn of(group: &Group) -> Gone {
        let cwd = std::fs::read_link(format!("/proc/{}/cwd", group.root))
            .ok()
            .map(|path| path.to_string_lossy().into_owned());
        Gone { name: group.name.clone(), argv: group.argv.clone(), cwd }
    }
}

pub fn parse_pressure(text: &str) -> Option<f64> {
    let line = text.lines().find(|line| line.starts_with("some"))?;
    line.split_whitespace().find_map(|part| part.strip_prefix("avg10=")).and_then(|value| value.parse().ok())
}

pub fn read_pressure() -> Option<f64> {
    parse_pressure(&std::fs::read_to_string("/proc/pressure/memory").ok()?)
}

pub fn power_of(read: impl Fn(&str) -> Option<String>) -> Option<look::Power> {
    let number = |name: &str| read(name).and_then(|text| text.trim().parse::<f64>().ok());
    let percent = number("capacity")?.clamp(0.0, 100.0) as u8;
    let status = read("status").unwrap_or_default();
    let charging = matches!(status.trim(), "Charging" | "Full");
    let watts = number("power_now")
        .map(|micro| micro / 1e6)
        .or_else(|| Some(number("current_now")? * number("voltage_now")? / 1e12))
        .filter(|watts| *watts > 0.05);
    let energy = number("energy_now")
        .map(|micro| micro / 1e6)
        .or_else(|| Some(number("charge_now")? * number("voltage_now")? / 1e12));
    let left_hours = match (charging, watts, energy) {
        (false, Some(watts), Some(energy)) if energy > 0.0 => Some(energy / watts),
        _ => None,
    };
    Some(look::Power { watts, left_hours, charging, percent })
}

pub fn read_power() -> Option<look::Power> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        if read("type").is_some_and(|kind| kind.trim() == "Battery")
            && read("scope").is_none_or(|scope| scope.trim() != "Device")
            && let Some(power) = power_of(read)
        {
            return Some(power);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    Background,
    System,
    Drifter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub pid: u32,
    pub ppid: u32,
    pub start: u64,
    pub name: String,
    pub cpu: f64,
    pub mem_kb: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub key: String,
    pub name: String,
    pub detail: String,
    pub icon: Option<String>,
    pub kind: Kind,
    pub root: u32,
    pub argv: Vec<String>,
    pub members: Vec<Member>,
    pub cpu: f64,
    pub mem_kb: u64,
    pub paused: bool,
    pub protected: bool,
    pub flatpak: bool,
}

impl Group {
    pub fn pids(&self) -> Vec<u32> {
        self.members.iter().map(|member| member.pid).collect()
    }

    pub fn can_stop(&self) -> bool {
        matches!(self.kind, Kind::App | Kind::Background) && !self.protected
    }

    pub fn score(&self, total_kb: u64) -> f64 {
        self.cpu + self.mem_kb as f64 / total_kb.max(1) as f64 * 60.0
    }

    pub fn impact(&self, total_kb: u64) -> Impact {
        if self.paused {
            return Impact::Paused;
        }
        match self.score(total_kb) {
            s if s > 45.0 => Impact::Heavy,
            s if s > 12.0 => Impact::Busy,
            _ => Impact::Calm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Impact {
    Calm,
    Busy,
    Heavy,
    Paused,
}

impl Impact {
    pub fn word(self) -> &'static str {
        match self {
            Impact::Calm => crate::lang::t("Calm"),
            Impact::Busy => crate::lang::t("Busy"),
            Impact::Heavy => crate::lang::t("Heavy"),
            Impact::Paused => crate::lang::t("Paused"),
        }
    }
}

pub fn friendly(word: &str) -> Option<&'static str> {
    let word = word.to_ascii_lowercase();
    let known: &[(&str, &str)] = &[
        ("pipewire-pulse", crate::lang::t("Sound")),
        ("pipewire", crate::lang::t("Sound")),
        ("wireplumber", crate::lang::t("Sound")),
        ("pulseaudio", crate::lang::t("Sound")),
        ("networkmanager", crate::lang::t("Network")),
        ("nm-applet", crate::lang::t("Network icon")),
        ("wpa_supplicant", crate::lang::t("Wi-Fi")),
        ("iwd", crate::lang::t("Wi-Fi")),
        ("bluetooth", crate::lang::t("Bluetooth")),
        ("bluetoothd", crate::lang::t("Bluetooth")),
        ("blueman-applet", crate::lang::t("Bluetooth icon")),
        ("systemd-journald", crate::lang::t("System log")),
        ("systemd-logind", crate::lang::t("Login")),
        ("systemd-udevd", crate::lang::t("Devices")),
        ("systemd-resolved", crate::lang::t("Name lookup")),
        ("systemd-timesyncd", crate::lang::t("Clock sync")),
        ("tracker-miner-fs-3", crate::lang::t("File search index")),
        ("localsearch-3", crate::lang::t("File search index")),
        ("xdg-desktop-portal", crate::lang::t("App portal")),
        ("xdg-desktop-portal-wlr", crate::lang::t("Screen sharing portal")),
        ("xdg-desktop-portal-gtk", crate::lang::t("File chooser portal")),
        ("xdg-document-portal", crate::lang::t("Document portal")),
        ("xdg-permission-store", crate::lang::t("Permission store")),
        ("dbus", crate::lang::t("Message bus")),
        ("dbus-daemon", crate::lang::t("Message bus")),
        ("dbus-broker", crate::lang::t("Message bus")),
        ("polkit", crate::lang::t("Permissions")),
        ("polkitd", crate::lang::t("Permissions")),
        ("cups", crate::lang::t("Printing")),
        ("cupsd", crate::lang::t("Printing")),
        ("upower", crate::lang::t("Battery info")),
        ("upowerd", crate::lang::t("Battery info")),
        ("udisks2", crate::lang::t("Disks")),
        ("udisksd", crate::lang::t("Disks")),
        ("power-profiles-daemon", crate::lang::t("Power mode")),
        ("accounts-daemon", crate::lang::t("User accounts")),
        ("cron", crate::lang::t("Scheduled tasks")),
        ("ssh", crate::lang::t("Remote login")),
        ("sshd", crate::lang::t("Remote login")),
        ("avahi-daemon", crate::lang::t("Network discovery")),
        ("modemmanager", crate::lang::t("Modems")),
        ("colord", crate::lang::t("Color profiles")),
        ("rtkit-daemon", crate::lang::t("Sound priority")),
        ("packagekit", crate::lang::t("Software updates")),
        ("packagekitd", crate::lang::t("Software updates")),
        ("xwayland-satellite", crate::lang::t("X11 apps")),
        ("xwayland", crate::lang::t("X11 apps")),
        ("gvfsd", crate::lang::t("Files service")),
        ("seatd", crate::lang::t("Seat")),
        ("systemd", crate::lang::t("Session manager")),
        ("at-spi-bus-launcher", crate::lang::t("Accessibility")),
        ("at-spi2-registryd", crate::lang::t("Accessibility")),
        ("gnome-keyring-daemon", crate::lang::t("Passwords")),
        ("snapd", crate::lang::t("Snap packages")),
        ("thermald", crate::lang::t("Temperature")),
        ("irqbalance", crate::lang::t("CPU balance")),
        ("rsyslog", crate::lang::t("System log")),
        ("chronyd", crate::lang::t("Clock sync")),
        ("firewalld", crate::lang::t("Firewall")),
    ];
    if word.starts_with("gvfs") {
        return Some(crate::lang::t("Files service"));
    }
    known.iter().find(|(key, _)| *key == word).map(|(_, name)| *name)
}

fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn match_app<'a>(proc: &Proc, apps: &'a [App]) -> Option<&'a App> {
    let mut names: Vec<&str> = Vec::new();
    if let Some(first) = proc.argv.first() {
        names.push(base(first));
    }
    names.push(proc.comm.as_str());
    let exec_name = |app: &App| -> Option<String> {
        let exec = app.exec.as_deref()?;
        let mut words = exec.split_whitespace().filter(|word| !word.contains('='));
        let mut first = words.next()?;
        if matches!(base(first), "env" | "flatpak" | "sh" | "bash") {
            first = words.find(|word| !word.starts_with('-'))?;
        }
        Some(base(first.trim_matches('"')).to_string())
    };
    for name in &names {
        if name.is_empty() {
            continue;
        }
        if let Some(app) = apps.iter().find(|app| exec_name(app).as_deref() == Some(*name)) {
            return Some(app);
        }
        if let Some(app) = apps.iter().find(|app| app.id.eq_ignore_ascii_case(name) || app.id.rsplit('.').next().is_some_and(|tail| tail.eq_ignore_ascii_case(name))) {
            return Some(app);
        }
    }
    None
}

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn cpu_share(before: u64, after: u64, total_before: u64, total_after: u64) -> f64 {
    let spent = after.saturating_sub(before) as f64;
    let total = total_after.saturating_sub(total_before) as f64;
    match total > 0.0 {
        true => (spent / total * 100.0).clamp(0.0, 100.0),
        false => 0.0,
    }
}

pub struct Grouping<'a> {
    pub shell: Option<u32>,
    pub me: u32,
    pub uid: Option<u32>,
    pub apps: &'a [App],
}

pub fn group(before: Option<&Snapshot>, now: &Snapshot, how: &Grouping) -> Vec<Group> {
    let by_pid: HashMap<u32, &Proc> = now.procs.iter().map(|proc| (proc.pid, proc)).collect();
    let was: HashMap<u32, u64> = before
        .map(|snap| snap.procs.iter().map(|proc| (proc.pid, proc.ticks)).collect())
        .unwrap_or_default();
    let total_before = before.map_or(now.total_ticks, |snap| snap.total_ticks);
    let reaper = |proc: &Proc| proc.comm == "systemd" && proc.pid != 1;
    let boundary = |pid: u32| {
        pid <= 1 || Some(pid) == how.shell || pid == how.me || by_pid.get(&pid).is_some_and(|proc| reaper(proc))
    };
    let top = |proc: &Proc| -> u32 {
        let mut at = proc;
        for _ in 0..64 {
            if Some(at.pid) == how.shell || boundary(at.ppid) || reaper(at) {
                return at.pid;
            }
            match by_pid.get(&at.ppid) {
                Some(up) => at = up,
                None => return at.pid,
            }
        }
        at.pid
    };
    let mut order: Vec<u32> = Vec::new();
    let mut members: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for proc in &now.procs {
        let root = top(proc);
        let list = members.entry(root).or_default();
        if list.is_empty() {
            order.push(root);
        }
        list.push(proc);
    }
    let mut groups: Vec<Group> = Vec::new();
    for root in order {
        let list = &members[&root];
        let Some(head) = by_pid.get(&root).copied() else {
            continue;
        };
        let cpu_of = |proc: &Proc| match was.get(&proc.pid) {
            Some(before) => cpu_share(*before, proc.ticks, total_before, now.total_ticks),
            None => 0.0,
        };
        let mut people: Vec<Member> = list
            .iter()
            .map(|proc| Member {
                pid: proc.pid,
                ppid: proc.ppid,
                start: proc.start,
                name: member_name(proc, head_program(head)),
                cpu: cpu_of(proc),
                mem_kb: proc.rss_kb,
            })
            .collect();
        people.sort_by(|a, b| (a.pid != root).cmp(&(b.pid != root)).then(b.cpu.total_cmp(&a.cpu)));
        let cpu: f64 = people.iter().map(|member| member.cpu).sum();
        let mem_kb: u64 = people.iter().map(|member| member.mem_kb).sum();
        let ours = how.uid.is_none() || head.uid == how.uid;
        let program = head_program(head);
        let flatpak = is_sandbox(head);
        let matched = match_app(head, how.apps).or_else(|| sandboxed_app(head, list, how.apps));
        let (kind, name, icon) = if Some(root) == how.shell {
            (Kind::Drifter, crate::lang::t("Drifter").to_string(), None)
        } else if program == SETTINGS_APP || head.comm == SETTINGS_APP {
            (Kind::App, crate::lang::t("Settings").to_string(), Some("preferences-system".to_string()))
        } else if program == ACTIVITY_APP || head.comm.starts_with("drifter-activi") {
            (Kind::App, crate::lang::t("Activity").to_string(), Some(art_icon()))
        } else if let (true, Some(app)) = (ours, matched) {
            (Kind::App, app.name.clone(), app.icon.clone())
        } else if let (true, true, Some(id)) = (ours, flatpak, flatpak_id(list)) {
            (Kind::App, title_case(id.rsplit('.').next().unwrap_or(&id)), Some(id))
        } else {
            let word = head.unit.clone().filter(|unit| !unit.starts_with("app-")).unwrap_or_else(|| program.clone());
            let name = friendly(&word)
                .or_else(|| friendly(&program))
                .map(str::to_string)
                .unwrap_or_else(|| title_case(&word));
            (if ours { Kind::Background } else { Kind::System }, name, None)
        };
        let paused = head.state == 'T';
        let detail = describe(kind, &people, root, &program, head.unit.as_deref(), paused);
        let detail = match (flatpak, paused) {
            (true, false) => crate::lang::tf("Flatpak · {}", &[&(detail)]),
            _ => detail,
        };
        let protected = root == how.me || (kind == Kind::Background && is_core(&program, head.unit.as_deref()));
        groups.push(Group {
            key: format!("{root}"),
            name,
            detail,
            icon,
            kind,
            root,
            argv: head.argv.clone(),
            members: people,
            cpu,
            mem_kb,
            paused,
            protected,
            flatpak,
        });
    }
    groups
}

fn head_program(head: &Proc) -> String {
    head.argv
        .first()
        .map(|first| base(first).to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| head.comm.clone())
}

fn member_name(proc: &Proc, root_program: String) -> String {
    let named = proc.argv.first().map(|first| base(first).to_string()).filter(|name| !name.is_empty());
    match named {
        Some(name) if name != root_program => name,
        _ if !proc.comm.is_empty() && proc.comm != root_program => proc.comm.clone(),
        Some(name) => name,
        None => proc.comm.clone(),
    }
}

pub fn is_core(program: &str, unit: Option<&str>) -> bool {
    let words = [program.to_ascii_lowercase(), unit.unwrap_or("").to_ascii_lowercase()];
    words.iter().any(|word| {
        word == "systemd"
            || word.starts_with("dbus")
            || word.starts_with("xwayland")
            || word.starts_with("xdg-")
            || word.starts_with("pipewire")
            || word == "wireplumber"
            || word.starts_with("at-spi")
            || word == "gnome-keyring-daemon"
            || word == "seatd"
            || word == "polkit-agent"
    })
}

fn is_sandbox(head: &Proc) -> bool {
    matches!(head_program(head).as_str(), "flatpak" | "bwrap" | "flatpak-spawn") || head.unit.as_deref().is_some_and(|unit| unit.starts_with("app-flatpak-"))
}

fn flatpak_id(list: &[&Proc]) -> Option<String> {
    list.iter().find_map(|proc| {
        let rest = proc.unit.as_deref()?.strip_prefix("app-flatpak-")?;
        Some(rest.rsplit_once('-').map_or(rest, |(id, _)| id).to_string())
    })
}

fn sandboxed_app<'a>(head: &Proc, list: &[&Proc], apps: &'a [App]) -> Option<&'a App> {
    if !is_sandbox(head) {
        return None;
    }
    let mut words: Vec<String> = Vec::new();
    for proc in list {
        if let Some(rest) = proc.unit.as_deref().and_then(|unit| unit.strip_prefix("app-flatpak-")) {
            let id = rest.rsplit_once('-').map_or(rest, |(id, _)| id);
            words.push(id.to_string());
        }
        words.extend(proc.argv.iter().cloned());
    }
    words.iter().find_map(|word| {
        let word = word.split_once("--command=").map_or(word.as_str(), |(_, rest)| rest);
        apps.iter().find(|app| app.id == word)
    })
}

fn describe(kind: Kind, people: &[Member], root: u32, program: &str, unit: Option<&str>, paused: bool) -> String {
    if paused {
        return crate::lang::t("Paused").to_string();
    }
    if kind == Kind::Drifter {
        return crate::lang::t("Draws your screen").to_string();
    }
    if let Some(job) = busiest_job(people, root, program) {
        return crate::lang::tf("Running {}", &[&(job)]);
    }
    let count = people.len();
    match kind {
        Kind::App => match count {
            1 => crate::lang::t("1 process").to_string(),
            n => format!("{n} processes"),
        },
        _ => {
            let word = unit.unwrap_or(program);
            match count {
                1 => word.to_string(),
                n => format!("{word} · {n} processes"),
            }
        }
    }
}

const SHELLS: [&str; 8] = ["sh", "bash", "zsh", "fish", "dash", "nu", "tmux", "screen"];

fn busiest_job(people: &[Member], root: u32, program: &str) -> Option<String> {
    let parent: HashMap<u32, u32> = people.iter().map(|member| (member.pid, member.ppid)).collect();
    let mut tree: HashMap<u32, f64> = HashMap::new();
    for member in people {
        let mut at = member.pid;
        for _ in 0..64 {
            *tree.entry(at).or_insert(0.0) += member.cpu;
            match parent.get(&at) {
                Some(up) if parent.contains_key(up) => at = *up,
                _ => break,
            }
        }
    }
    people
        .iter()
        .filter(|member| member.pid != root && member.name != program && !SHELLS.contains(&member.name.as_str()))
        .filter(|member| tree.get(&member.pid).copied().unwrap_or(0.0) >= 5.0)
        .max_by(|a, b| {
            let (x, y) = (tree[&a.pid], tree[&b.pid]);
            x.total_cmp(&y).then(b.pid.cmp(&a.pid))
        })
        .map(|member| member.name.clone())
}

pub fn sorted(groups: &mut [Group], by: Column, total_kb: u64) {
    match by {
        Column::Name => groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        Column::Impact => groups.sort_by(|a, b| b.score(total_kb).total_cmp(&a.score(total_kb))),
        Column::Cpu => groups.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
        Column::Memory => groups.sort_by(|a, b| b.mem_kb.cmp(&a.mem_kb)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    Name,
    Impact,
    Cpu,
    Memory,
}

pub fn memory_words(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    match mb >= 1000.0 {
        true => format!("{:.1} GB", mb / 1024.0),
        false => format!("{} MB", mb.round() as u64),
    }
}

pub fn percent_words(value: f64) -> String {
    match value {
        v if v <= 0.0 => "0%".to_string(),
        v if v < 10.0 => format!("{v:.1}%"),
        v => format!("{}%", v.round() as u64),
    }
}

pub fn tone(ink: [u8; 3], percent: f64) -> [u8; 3] {
    crate::glance::tone(ink, percent / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, comm: &str, ticks: u64, rss_kb: u64) -> Proc {
        Proc {
            pid,
            ppid,
            comm: comm.to_string(),
            state: 'S',
            ticks,
            start: 0,
            rss_kb,
            uid: Some(1000),
            argv: vec![format!("/usr/bin/{comm}")],
            unit: None,
        }
    }

    fn app(id: &str, name: &str, exec: &str) -> App {
        App {
            id: id.to_string(),
            name: name.to_string(),
            icon: Some(id.to_string()),
            exec: Some(exec.to_string()),
            words: String::new(),
            categories: Vec::new(),
            terminal: false,
        }
    }

    #[test]
    fn a_stat_line_with_spaces_and_brackets_in_the_name_reads() {
        let line = "4242 (Web Content (1)) S 4200 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0 1234 0 0";
        let (pid, comm, state, ppid, ticks) = parse_stat(line).unwrap();
        assert_eq!((pid, comm.as_str(), state, ppid, ticks), (4242, "Web Content (1)", 'S', 4200, 300));
    }

    #[test]
    fn statm_counts_resident_pages_in_kilobytes() {
        assert_eq!(parse_statm("50000 1200 300 10 0 900 0", 4), Some(4800));
    }

    #[test]
    fn the_unit_comes_from_the_last_cgroup_part() {
        assert_eq!(unit_of("0::/system.slice/NetworkManager.service\n").as_deref(), Some("NetworkManager"));
        assert_eq!(unit_of("0::/user.slice/user-1000.slice/user@1000.service/session.slice/pipewire.service").as_deref(), Some("pipewire"));
        assert_eq!(unit_of("0::/user.slice/user-1000.slice/session-2.scope").as_deref(), None);
        assert_eq!(unit_of("0::/system.slice/getty@tty1.service").as_deref(), Some("getty"));
    }

    #[test]
    fn argv_splits_on_nul_bytes() {
        assert_eq!(parse_argv(b"foot\0-e\0htop\0"), vec!["foot", "-e", "htop"]);
    }

    fn machine(ticks: &[(u32, u32, &str, u64, u64)], total: u64) -> Snapshot {
        Snapshot {
            procs: ticks.iter().map(|(pid, ppid, comm, t, rss)| proc(*pid, *ppid, comm, *t, *rss)).collect(),
            total_ticks: total,
            busy_ticks: 0,
            memory: Memory { total_kb: 16 * 1024 * 1024, available_kb: 8 * 1024 * 1024, ..Memory::default() },
            cores: 8,
        }
    }

    #[test]
    fn children_of_drifter_become_apps_and_the_rest_stays_with_its_top() {
        let apps = vec![app("org.mozilla.firefox", "Firefox", "firefox %u"), app("foot", "Foot", "foot")];
        let before = machine(
            &[(1, 0, "systemd", 0, 0), (900, 1, "drifter", 0, 200_000), (2000, 900, "firefox", 100, 500_000), (2010, 2000, "Web Content", 100, 300_000), (3000, 900, "foot", 0, 30_000), (3010, 3000, "cargo", 0, 100_000)],
            1000,
        );
        let after = machine(
            &[(1, 0, "systemd", 0, 0), (900, 1, "drifter", 10, 200_000), (2000, 900, "firefox", 150, 500_000), (2010, 2000, "Web Content", 200, 300_000), (3000, 900, "foot", 0, 30_000), (3010, 3000, "cargo", 400, 100_000)],
            2000,
        );
        let how = Grouping { shell: Some(900), me: 4000, uid: Some(1000), apps: &apps };
        let groups = group(Some(&before), &after, &how);
        let find = |name: &str| groups.iter().find(|group| group.name == name).unwrap();
        let fox = find("Firefox");
        assert_eq!(fox.kind, Kind::App);
        assert_eq!(fox.members.len(), 2);
        assert!((fox.cpu - 15.0).abs() < 1e-9, "{}", fox.cpu);
        assert_eq!(fox.mem_kb, 800_000);
        let term = find("Foot");
        assert_eq!(term.detail, "Running cargo");
        assert!(find("Firefox").members.iter().any(|member| member.name == "Web Content"), "a child that reuses the app binary goes by its own name");
        assert_eq!(find("Drifter").kind, Kind::Drifter);
        assert_eq!(find("Drifter").members.len(), 1, "apps are not counted inside Drifter");
    }

    #[test]
    fn a_service_of_another_user_is_system_and_cannot_be_stopped() {
        let mut snap = machine(&[(640, 1, "NetworkManager", 0, 20_000)], 10);
        snap.procs[0].uid = Some(0);
        snap.procs[0].unit = Some("NetworkManager".to_string());
        let groups = group(None, &snap, &Grouping { shell: None, me: 1, uid: Some(1000), apps: &[] });
        assert_eq!(groups[0].kind, Kind::System);
        assert_eq!(groups[0].name, "Network");
        assert!(!groups[0].can_stop());
    }

    #[test]
    fn a_stopped_process_shows_as_paused() {
        let mut snap = machine(&[(900, 1, "drifter", 0, 1), (2000, 900, "blender", 0, 3_000_000)], 10);
        snap.procs[1].state = 'T';
        let groups = group(None, &snap, &Grouping { shell: Some(900), me: 1, uid: Some(1000), apps: &[] });
        let blender = groups.iter().find(|group| group.root == 2000).unwrap();
        assert!(blender.paused);
        assert_eq!(blender.impact(16 * 1024 * 1024), Impact::Paused);
        assert_eq!(blender.detail, "Paused");
    }

    #[test]
    fn impact_grows_with_cpu_and_memory() {
        let mut group = Group {
            key: "1".into(),
            name: "x".into(),
            detail: String::new(),
            icon: None,
            kind: Kind::App,
            root: 1,
            argv: Vec::new(),
            members: Vec::new(),
            cpu: 1.0,
            mem_kb: 100_000,
            paused: false,
            protected: false,
            flatpak: false,
        };
        let total = 16 * 1024 * 1024;
        assert_eq!(group.impact(total), Impact::Calm);
        group.cpu = 20.0;
        assert_eq!(group.impact(total), Impact::Busy);
        group.cpu = 80.0;
        assert_eq!(group.impact(total), Impact::Heavy);
    }

    #[test]
    fn a_build_in_a_terminal_reads_as_the_build_not_its_leaf() {
        let apps = vec![app("foot", "Foot", "foot")];
        let mut procs = vec![(1, 0, "systemd", 0, 0), (900, 1, "drifter", 0, 1), (3000, 900, "foot", 0, 1), (3001, 3000, "bash", 0, 1), (3010, 3001, "cargo", 10, 1)];
        procs.extend((0..4).map(|at| (3020 + at, 3010, "rustc", 0, 1)));
        let before = machine(&procs, 1000);
        let mut later = procs.clone();
        for proc in later.iter_mut().filter(|proc| proc.2 == "rustc") {
            proc.3 = 200;
        }
        let after = machine(&later, 2000);
        let groups = group(Some(&before), &after, &Grouping { shell: Some(900), me: 5, uid: Some(1000), apps: &apps });
        let foot = groups.iter().find(|group| group.name == "Foot").unwrap();
        assert_eq!(foot.detail, "Running cargo");
    }

    #[test]
    fn a_flatpak_app_is_an_app_with_its_own_name() {
        let apps = vec![app("org.gnome.Calculator", "Calculator", "/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=gnome-calculator org.gnome.Calculator")];
        let mut snap = machine(&[(900, 1, "drifter", 0, 1), (4000, 900, "bwrap", 0, 1), (4001, 4000, "gnome-calculator", 0, 50_000)], 10);
        snap.procs[1].argv = vec!["bwrap".into(), "--args".into(), "40".into(), "--".into(), "gnome-calculator".into()];
        snap.procs[2].unit = Some("app-flatpak-org.gnome.Calculator-1234".into());
        let groups = group(None, &snap, &Grouping { shell: Some(900), me: 5, uid: Some(1000), apps: &apps });
        let calc = groups.iter().find(|group| group.root == 4000).unwrap();
        assert!(calc.detail.starts_with("Flatpak · "), "{}", calc.detail);
        assert_eq!((calc.kind, calc.name.as_str()), (Kind::App, "Calculator"));
        assert!(calc.flatpak);
        assert!(!groups.iter().find(|group| group.root == 900).unwrap().flatpak);
    }

    #[test]
    fn session_services_and_activity_itself_cannot_be_stopped_here() {
        let mut snap = machine(&[(1, 0, "systemd", 0, 0), (700, 1, "systemd", 0, 1), (710, 700, "dbus-broker", 0, 1), (720, 700, "tracker-miner-fs-3", 0, 1), (900, 1, "drifter", 0, 1), (950, 900, "drifter-activity", 0, 1)], 10);
        snap.procs[1].argv = vec!["/usr/lib/systemd/systemd".into(), "--user".into()];
        let groups = group(None, &snap, &Grouping { shell: Some(900), me: 950, uid: Some(1000), apps: &[] });
        let can = |root: u32| groups.iter().find(|group| group.root == root).unwrap().can_stop();
        assert!(!can(700), "the session manager");
        assert!(!can(710), "the message bus");
        assert!(can(720), "an ordinary background service");
        assert!(!can(950), "Activity cannot pause itself");
    }

    #[test]
    fn cores_are_counted_once_per_physical_core() {
        let info = "processor\t: 0\nphysical id\t: 0\ncore id\t: 0\n\nprocessor\t: 1\nphysical id\t: 0\ncore id\t: 1\n\nprocessor\t: 2\nphysical id\t: 0\ncore id\t: 0\n\nprocessor\t: 3\nphysical id\t: 0\ncore id\t: 1\n";
        assert_eq!(physical_cores(info), Some(2));
        assert_eq!(physical_cores("processor : 0\n"), None);
    }

    #[test]
    fn memory_pressure_reads_the_ten_second_average() {
        let text = "some avg10=3.25 avg60=1.00 avg300=0.10 total=123\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        assert_eq!(parse_pressure(text), Some(3.25));
    }

    #[test]
    fn a_battery_on_its_way_down_says_how_long_it_lasts() {
        let files = |name: &str| match name {
            "capacity" => Some("82\n".to_string()),
            "status" => Some("Discharging\n".to_string()),
            "power_now" => Some("14200000\n".to_string()),
            "energy_now" => Some("45000000\n".to_string()),
            _ => None,
        };
        let power = power_of(files).unwrap();
        assert_eq!(power.percent, 82);
        assert!((power.watts.unwrap() - 14.2).abs() < 1e-9);
        assert!((power.left_hours.unwrap() - 45.0 / 14.2).abs() < 1e-9);
        let charging = |name: &str| match name {
            "status" => Some("Charging".to_string()),
            other => files(other),
        };
        assert_eq!(power_of(charging).unwrap().left_hours, None);
    }

    #[test]
    fn numbers_read_as_people_say_them() {
        assert_eq!(memory_words(2_200_000), "2.1 GB");
        assert_eq!(memory_words(150 * 1024), "150 MB");
        assert_eq!(percent_words(3.25), "3.2%");
        assert_eq!(percent_words(42.6), "43%");
        assert_eq!(percent_words(0.0), "0%");
    }

    #[test]
    fn an_app_is_found_by_its_exec_or_its_id() {
        let apps = vec![app("firefox-esr", "Firefox ESR", "/usr/lib/firefox-esr/firefox-esr %u"), app("org.gnome.Nautilus", "Files", "nautilus --new-window %U")];
        let mut fox = proc(1, 0, "firefox-esr", 0, 0);
        fox.argv = vec!["/usr/lib/firefox-esr/firefox-esr".into()];
        assert_eq!(match_app(&fox, &apps).map(|app| app.name.as_str()), Some("Firefox ESR"));
        let files = proc(2, 0, "nautilus", 0, 0);
        assert_eq!(match_app(&files, &apps).map(|app| app.name.as_str()), Some("Files"));
    }
}
