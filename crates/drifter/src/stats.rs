use std::time::{Duration, Instant};

use drifter_core::stats::{self, CpuTimes, Stats};

const EVERY: Duration = Duration::from_secs(1);

pub struct Sampler {
    cpu: CpuTimes,
    taken: Option<Instant>,
    last: Option<Stats>,
}

impl Sampler {
    pub fn new() -> Sampler {
        Sampler {
            cpu: read_cpu().unwrap_or_default(),
            taken: None,
            last: None,
        }
    }

    pub fn read(&mut self) -> Option<Stats> {
        if self.taken.is_some_and(|taken| taken.elapsed() < EVERY) {
            return self.last;
        }
        self.taken = Some(Instant::now());

        let cpu = read_cpu()?;
        let share = stats::cpu_percent(self.cpu, cpu);
        self.cpu = cpu;
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        self.last = Some(Stats {
            cpu: share,
            ram: stats::ram_percent(&meminfo)?,
            disk: disk()?,
        });
        self.last
    }
}

fn read_cpu() -> Option<CpuTimes> {
    stats::cpu_times(&std::fs::read_to_string("/proc/stat").ok()?)
}

fn disk() -> Option<u8> {
    let mut info: libc::statvfs = unsafe { std::mem::zeroed() };
    let done = unsafe { libc::statvfs(c"/".as_ptr(), &mut info) };
    if done != 0 {
        return None;
    }
    Some(stats::disk_percent(
        info.f_blocks as u64,
        info.f_bfree as u64,
        info.f_bavail as u64,
    ))
}
