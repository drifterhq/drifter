use std::collections::HashSet;
use std::ffi::CString;
use std::sync::{Arc, Mutex};

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use drifter_core::TrayConfig;
use drifter_core::activity::{self, Group, Grouping, Snapshot, hour};
use drifter_core::stats::{self, CpuTimes, Metric, Mount};

pub struct Sampler {
    wanted: Arc<Mutex<Vec<Metric>>>,
    latest: Arc<Mutex<Vec<(String, u8)>>>,
    watching: Arc<AtomicBool>,
    groups: Arc<Mutex<Option<(Vec<Group>, u64)>>>,
}

#[derive(Default)]
struct Apps {
    before: Option<Snapshot>,
    hour_base: Option<(Snapshot, Instant)>,
    apps: Vec<drifter_core::desktop::App>,
    apps_at: Option<Instant>,
}

impl Apps {
    fn step(&mut self, watching: bool, out: &Mutex<Option<(Vec<Group>, u64)>>) {
        let hour_due = self
            .hour_base
            .as_ref()
            .is_none_or(|(_, at)| at.elapsed() >= Duration::from_secs(hour::EVERY_SECONDS));
        if !watching && !hour_due {
            self.before = None;
            return;
        }
        if self.apps_at.is_none_or(|at| at.elapsed() >= Duration::from_secs(300)) {
            self.apps = drifter_core::desktop::installed();
            self.apps_at = Some(Instant::now());
        }
        let snapshot = activity::read_snapshot();
        let me = std::process::id();
        let how = Grouping { shell: Some(me), me, uid: activity::own_uid(), apps: &self.apps };
        if watching {
            let groups = activity::group(self.before.as_ref(), &snapshot, &how);
            if self.before.is_some()
                && let Ok(mut held) = out.lock()
            {
                *held = Some((groups, snapshot.memory.total_kb));
            }
        }
        if hour_due {
            if let Some((base, _)) = self.hour_base.as_ref() {
                let groups = activity::group(Some(base), &snapshot, &how);
                let cpu = activity::cpu_share(base.busy_ticks, snapshot.busy_ticks, base.total_ticks, snapshot.total_ticks);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.as_secs())
                    .unwrap_or(0);
                let reading = hour::Reading::take(now, cpu, &snapshot.memory, &groups);
                let path = drifter_core::Config::state_dir().join(hour::FILE);
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let _ = std::fs::create_dir_all(drifter_core::Config::state_dir());
                if let Err(error) = std::fs::write(&path, hour::append(&text, &reading)) {
                    log!("drifter: cannot keep the last hour: {error}");
                }
            }
            self.hour_base = Some((snapshot.clone(), Instant::now()));
        }
        self.before = Some(snapshot);
    }
}

impl Sampler {
    pub fn new(config: &TrayConfig) -> Sampler {
        let wanted = Arc::new(Mutex::new(chosen(config)));
        let latest = Arc::new(Mutex::new(Vec::new()));
        let watching = Arc::new(AtomicBool::new(false));
        let groups = Arc::new(Mutex::new(None));
        let (asked, told, watched, grouped) = (wanted.clone(), latest.clone(), watching.clone(), groups.clone());
        let spawned = std::thread::Builder::new()
            .name("drifter-stats".to_string())
            .spawn(move || {
                let mut reader = Reader::default();
                let mut apps = Apps::default();
                loop {
                    let metrics = asked.lock().map(|held| held.clone()).unwrap_or_default();
                    let fields = reader.read(&metrics);
                    if let Ok(mut held) = told.lock() {
                        *held = fields;
                    }
                    apps.step(watched.load(Ordering::Relaxed), &grouped);
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            });
        if let Err(error) = spawned {
            log!("drifter: no system info: {error}");
        }
        Sampler { wanted, latest, watching, groups }
    }

    pub fn watch(&self, on: bool) {
        if self.watching.swap(on, Ordering::Relaxed) != on
            && !on
            && let Ok(mut held) = self.groups.lock()
        {
            *held = None;
        }
    }

    pub fn groups(&self) -> Option<(Vec<Group>, u64)> {
        self.groups.lock().ok().and_then(|held| held.clone())
    }

    pub fn follow(&self, config: &TrayConfig) {
        if let Ok(mut held) = self.wanted.lock() {
            *held = chosen(config);
        }
        if let Ok(mut held) = self.latest.lock() {
            held.clear();
        }
    }

    pub fn read(&self) -> Vec<(String, u8)> {
        self.latest
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
    }
}

fn chosen(config: &TrayConfig) -> Vec<Metric> {
    match config.stats {
        true => config
            .metrics
            .iter()
            .filter_map(|metric| Metric::parse(metric).ok())
            .collect(),
        false => Vec::new(),
    }
}

#[derive(Default)]
struct Reader {
    last: Option<CpuTimes>,
    missing: HashSet<String>,
}

impl Reader {
    fn read(&mut self, metrics: &[Metric]) -> Vec<(String, u8)> {
        let mut fields = Vec::new();
        let mut mounts: Option<Vec<Mount>> = None;
        for metric in metrics {
            let field = match &metric {
                Metric::Cpu => self.cpu().map(|value| ("CPU".to_string(), value)),
                Metric::Ram => ram().map(|value| ("RAM".to_string(), value)),
                Metric::Battery => battery().map(|(label, value)| (label.to_string(), value)),
                Metric::Disk(name) => {
                    let mounts = mounts.get_or_insert_with(|| {
                        std::fs::read_to_string("/proc/self/mountinfo")
                            .map(|text| stats::mounts(&text))
                            .unwrap_or_default()
                    });
                    let field = disk(mounts, name);
                    if field.is_none() && self.missing.insert(name.clone()) {
                        log!("drifter: tray metric {name:?} is not a mounted disk");
                    }
                    field
                }
            };
            fields.extend(field);
        }
        fields
    }

    fn cpu(&mut self) -> Option<u8> {
        let now = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|text| stats::cpu_times(&text))?;
        let before = self
            .last
            .replace(now)
            .unwrap_or(CpuTimes { busy: 0, total: 0 });
        Some(stats::cpu_percent(before, now))
    }
}

pub(crate) fn battery() -> Option<(&'static str, u8)> {
    let read = |dir: &std::path::Path, name: &str| std::fs::read_to_string(dir.join(name)).ok();
    let cells: Vec<stats::Cell> = std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let kind = read(&dir, "type")?;
            let full = read(&dir, "energy_full").or_else(|| read(&dir, "charge_full"));
            let now = read(&dir, "energy_now").or_else(|| read(&dir, "charge_now"));
            stats::read_cell(
                &kind,
                read(&dir, "capacity").as_deref(),
                (full.as_deref(), now.as_deref()),
                &read(&dir, "status").unwrap_or_default(),
            )
        })
        .collect();
    stats::battery(&cells)
}

fn ram() -> Option<u8> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| stats::ram_percent(&text))
}

fn disk(mounts: &[Mount], name: &str) -> Option<(String, u8)> {
    let mount = stats::find_disk(mounts, name, kernel)?;
    let used = used(&mount.point)?;
    let label = stats::disk_label(kernel(mount).as_deref(), &mount.source);
    Some((label, used))
}

fn kernel(mount: &Mount) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/dev/block/{}", mount.device)).ok()?;
    Some(link.file_name()?.to_string_lossy().into_owned())
}

fn used(point: &str) -> Option<u8> {
    let path = CString::new(point).ok()?;
    let mut info: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut info) } != 0 {
        return None;
    }
    Some(stats::disk_percent(
        info.f_blocks as u64,
        info.f_bfree as u64,
        info.f_bavail as u64,
    ))
}
