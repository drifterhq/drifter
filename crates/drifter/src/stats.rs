use std::collections::HashSet;
use std::ffi::CString;

use drifter_core::TrayConfig;
use drifter_core::stats::{self, CpuTimes, Metric, Mount};

/// Reads the metrics the config asks for. CPU needs two readings, so the
/// last one is kept.
#[derive(Default)]
pub struct Sampler {
    metrics: Vec<Metric>,
    last: Option<CpuTimes>,
    missing: HashSet<String>,
}

impl Sampler {
    pub fn new(config: &TrayConfig) -> Sampler {
        let metrics = match config.stats {
            true => config
                .metrics
                .iter()
                .filter_map(|metric| Metric::parse(metric).ok())
                .collect(),
            false => Vec::new(),
        };
        Sampler {
            metrics,
            ..Sampler::default()
        }
    }

    pub fn read(&mut self) -> Vec<(String, u8)> {
        let mut fields = Vec::new();
        let mut mounts: Option<Vec<Mount>> = None;
        for metric in self.metrics.clone() {
            let field = match &metric {
                Metric::Cpu => self.cpu().map(|value| ("CPU".to_string(), value)),
                Metric::Ram => ram().map(|value| ("RAM".to_string(), value)),
                Metric::Disk(name) => {
                    let mounts = mounts.get_or_insert_with(|| {
                        std::fs::read_to_string("/proc/self/mountinfo")
                            .map(|text| stats::mounts(&text))
                            .unwrap_or_default()
                    });
                    let field = disk(mounts, name);
                    if field.is_none() && self.missing.insert(name.clone()) {
                        eprintln!("drifter: tray metric {name:?} is not a mounted disk");
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

/// The kernel's name for the block device behind a mount, like "sda3".
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
