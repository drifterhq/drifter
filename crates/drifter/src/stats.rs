use std::ffi::CString;

use drifter_core::stats::{self, CpuTimes};

/// Reads CPU, RAM and disk. CPU needs two readings, so the last one is kept.
#[derive(Default)]
pub struct Sampler {
    last: Option<CpuTimes>,
}

impl Sampler {
    pub fn read(&mut self) -> Option<String> {
        let now = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|text| stats::cpu_times(&text))?;
        let before = self
            .last
            .replace(now)
            .unwrap_or(CpuTimes { busy: 0, total: 0 });
        let cpu = stats::cpu_percent(before, now);
        let ram = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|text| stats::ram_percent(&text))?;
        let disk = disk("/")?;
        Some(stats::text(cpu, ram, disk))
    }
}

fn disk(path: &str) -> Option<u8> {
    let path = CString::new(path).ok()?;
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
