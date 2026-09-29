/// Busy and total time for all cores, in the ticks /proc/stat counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// The widest the text can ever be, so the space it takes never changes.
pub const WIDEST: &str = "CPU 100%   RAM 100%   DSK 100%";

pub fn text(cpu: u8, ram: u8, disk: u8) -> String {
    format!("CPU {cpu}%   RAM {ram}%   DSK {disk}%")
}

/// Reads the first line of /proc/stat. Idle and iowait are the idle time,
/// guest time is already counted inside user and nice, so it is left out.
pub fn cpu_times(stat: &str) -> Option<CpuTimes> {
    let line = stat.lines().find(|line| line.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|field| field.parse().ok())
        .collect::<Option<_>>()?;
    if fields.len() < 4 {
        return None;
    }
    let total: u64 = fields.iter().sum();
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    Some(CpuTimes {
        busy: total.saturating_sub(idle),
        total,
    })
}

pub fn cpu_percent(before: CpuTimes, after: CpuTimes) -> u8 {
    let total = after.total.saturating_sub(before.total);
    let busy = after.busy.saturating_sub(before.busy);
    percent(busy, total)
}

/// Used memory is the total minus what the kernel says is available.
pub fn ram_percent(meminfo: &str) -> Option<u8> {
    let field = |name: &str| {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some(percent(total.saturating_sub(available), total))
}

/// Counts the way df does: blocks kept for root are not part of the size,
/// and a part used rounds up.
pub fn disk_percent(blocks: u64, free: u64, available: u64) -> u8 {
    let used = blocks.saturating_sub(free);
    let size = used + available;
    if size == 0 {
        return 0;
    }
    (used * 100).div_ceil(size).min(100) as u8
}

fn percent(part: u64, whole: u64) -> u8 {
    if whole == 0 {
        return 0;
    }
    ((part * 100 + whole / 2) / whole).min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 20 30 800 50 0 0 0 0 0\ncpu0 50 10 15 400 25 0 0 0 0 0\nintr 1\n";

    #[test]
    fn the_cpu_line_counts_idle_and_iowait_as_idle() {
        let times = cpu_times(STAT).unwrap();
        assert_eq!(times.total, 1000);
        assert_eq!(times.busy, 150);
    }

    #[test]
    fn guest_time_is_not_counted_twice() {
        let stat = "cpu  100 0 0 100 0 0 0 0 40 0\n";
        assert_eq!(cpu_times(stat).unwrap().total, 200);
    }

    #[test]
    fn the_per_core_lines_are_skipped() {
        let stat = "cpu0 1 1 1 1\ncpu  2 0 0 2\n";
        assert_eq!(cpu_times(stat).unwrap().total, 4);
    }

    #[test]
    fn a_broken_stat_file_gives_nothing() {
        assert_eq!(cpu_times(""), None);
        assert_eq!(cpu_times("cpu  1 2\n"), None);
        assert_eq!(cpu_times("cpu  a b c d\n"), None);
    }

    #[test]
    fn cpu_is_the_busy_share_between_two_readings() {
        let before = CpuTimes {
            busy: 100,
            total: 1000,
        };
        let after = CpuTimes {
            busy: 175,
            total: 1100,
        };
        assert_eq!(cpu_percent(before, after), 75);
    }

    #[test]
    fn no_time_passed_is_zero_not_a_panic() {
        let same = CpuTimes { busy: 5, total: 10 };
        assert_eq!(cpu_percent(same, same), 0);
    }

    #[test]
    fn ram_is_total_minus_available() {
        let meminfo = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    12000000 kB\n";
        assert_eq!(ram_percent(meminfo), Some(25));
    }

    #[test]
    fn ram_without_available_gives_nothing() {
        assert_eq!(ram_percent("MemTotal: 100 kB\n"), None);
    }

    #[test]
    fn disk_leaves_out_the_root_reserve_and_rounds_up_like_df() {
        assert_eq!(disk_percent(1000, 400, 350), 64);
        assert_eq!(disk_percent(1000, 999, 999), 1);
        assert_eq!(disk_percent(1000, 1000, 1000), 0);
    }

    #[test]
    fn an_empty_disk_is_zero_not_a_panic() {
        assert_eq!(disk_percent(0, 0, 0), 0);
    }

    #[test]
    fn the_text_reads_as_three_percentages() {
        assert_eq!(text(7, 42, 63), "CPU 7%   RAM 42%   DSK 63%");
    }

    #[test]
    fn the_widest_text_is_every_value_at_one_hundred() {
        assert_eq!(text(100, 100, 100), WIDEST);
    }
}
