use std::ffi::CString;
use std::time::Duration;

use drifter_core::activity::hour::{self, Reading};
use drifter_core::activity::look::{Disk, Power};
use drifter_core::activity::{self, Group, Grouping, Memory, Snapshot};
use drifter_core::stats::{self, Mount};
use drifter_core::{Config, desktop};
use smithay_client_toolkit::reexports::calloop::channel::Sender;

pub struct Fresh {
    pub groups: Vec<Group>,
    pub memory: Memory,
    pub cpu: f64,
    pub cores: usize,
    pub pressure: Option<f64>,
    pub power: Option<Option<Power>>,
    pub disk: Option<Option<Disk>>,
    pub hour: Option<Vec<Reading>>,
    pub now: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

pub fn start(out: Sender<Fresh>) {
    let spawned = std::thread::Builder::new().name("activity-probe".to_string()).spawn(move || {
        let me = std::process::id();
        let uid = activity::own_uid();
        let mut apps = desktop::installed();
        let mut before: Option<Snapshot> = None;
        let mut tick: u64 = 0;
        loop {
            let snapshot = activity::read_snapshot();
            let shell = activity::find_shell(&snapshot, me);
            if tick % 300 == 299 {
                apps = desktop::installed();
            }
            let how = Grouping { shell, me, uid, apps: &apps };
            let groups = activity::group(before.as_ref(), &snapshot, &how);
            let cpu = match before.as_ref() {
                Some(was) => activity::cpu_share(was.busy_ticks, snapshot.busy_ticks, was.total_ticks, snapshot.total_ticks),
                None => 0.0,
            };
            let hour = (tick % 10 == 0).then(read_hour);
            let fresh = Fresh {
                groups: match before.is_some() {
                    true => groups,
                    false => Vec::new(),
                },
                memory: snapshot.memory.clone(),
                cpu,
                cores: snapshot.cores,
                pressure: activity::read_pressure(),
                power: (tick % 5 == 0).then(activity::read_power),
                disk: (tick % 30 == 0).then(root_disk),
                hour,
                now: now(),
            };
            let first = before.is_none();
            before = Some(snapshot);
            if !first && out.send(fresh).is_err() {
                return;
            }
            if first {
                std::thread::sleep(Duration::from_millis(250));
                continue;
            }
            tick += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    if let Err(error) = spawned {
        log!("drifter-activity: cannot start the reader: {error}");
    }
}

pub fn read_hour() -> Vec<Reading> {
    let text = std::fs::read_to_string(Config::state_dir().join(hour::FILE)).unwrap_or_default();
    hour::read(&text, now())
}

fn kernel(mount: &Mount) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/dev/block/{}", mount.device)).ok()?;
    Some(link.file_name()?.to_string_lossy().into_owned())
}

fn root_disk() -> Option<Disk> {
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").map(|text| stats::mounts(&text)).ok()?;
    let mount = stats::find_disk(&mounts, "/", kernel)?;
    let path = CString::new(mount.point.as_str()).ok()?;
    let mut info: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut info) } != 0 {
        return None;
    }
    let used = stats::disk_percent(info.f_blocks as u64, info.f_bfree as u64, info.f_bavail as u64);
    let size_gb = info.f_blocks as f64 * info.f_frsize as f64 / 1e9;
    Some(Disk {
        label: stats::disk_label(kernel(mount).as_deref(), &mount.source),
        used,
        size_gb,
    })
}
