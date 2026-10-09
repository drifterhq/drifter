use crate::image::Image;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

const WIDEST: &str = "100%";
const SPACE: &str = " ";
const BETWEEN: &str = "   ";

pub fn paint(font: &Font, fields: &[(&str, u8)], color: [u8; 3]) -> Image {
    let slot = font.width_of(WIDEST);
    let space = font.width_of(SPACE);
    let between = font.width_of(BETWEEN);
    let mut pieces = Vec::new();
    let mut pen = 0.0;
    for (index, (label, value)) in fields.iter().enumerate() {
        if index > 0 {
            pen += between;
        }
        pieces.push((pen, font.paint(label, color), None));
        pen += font.width_of(label) + space + slot;
        let percent = (pen - font.width_of("%")).round();
        let digits = value.to_string();
        pieces.push((percent, font.paint("%", color), None));
        pieces.push((
            percent - font.width_of(&digits),
            font.paint(&digits, color),
            Some(percent as i64),
        ));
    }

    let dense = crate::paint::density();
    let width = crate::paint::dense(pen.ceil().max(1.0));
    let height = crate::paint::dense(font.line_height().ceil().max(1.0));
    let mut canvas = Image {
        width,
        height,
        rgba: vec![0; (width as usize) * (height as usize) * 4],
    };
    for (x, piece, right) in pieces {
        let right = right.map(|right| (right as f64 * dense).round() as i64);
        stamp(&mut canvas, &piece, (x.round() * dense).round() as i64, right);
    }
    canvas
}

fn stamp(canvas: &mut Image, piece: &Image, left: i64, right: Option<i64>) {
    let right = right
        .unwrap_or(i64::from(canvas.width))
        .min(i64::from(canvas.width));
    for y in 0..piece.height.min(canvas.height) {
        for x in 0..piece.width {
            let to = left + i64::from(x);
            if to < 0 || to >= right {
                continue;
            }
            let from = ((y * piece.width + x) * 4) as usize;
            if piece.rgba[from + 3] == 0 {
                continue;
            }
            let at = ((i64::from(y) * i64::from(canvas.width) + to) * 4) as usize;
            canvas.rgba[at..at + 4].copy_from_slice(&piece.rgba[from..from + 4]);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Metric {
    Cpu,
    Ram,
    Battery,
    Disk(String),
}

impl Metric {
    pub fn parse(text: &str) -> Result<Metric, String> {
        match text.trim() {
            "cpu" => Ok(Metric::Cpu),
            "ram" => Ok(Metric::Ram),
            "battery" => Ok(Metric::Battery),
            "" => Err(
                "a tray metric is empty. use cpu, ram, battery, a mount point or a device".into(),
            ),
            disk => match disk.strip_prefix("/dev/") {
                Some(device) => Ok(Metric::Disk(
                    device.rsplit('/').next().unwrap_or(device).to_string(),
                )),
                None => Ok(Metric::Disk(disk.to_string())),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: String,
    pub point: String,
    pub source: String,
}

pub fn mounts(mountinfo: &str) -> Vec<Mount> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (head, tail) = line.split_once(" - ")?;
            let head: Vec<&str> = head.split_whitespace().collect();
            Some(Mount {
                device: head.get(2)?.to_string(),
                point: head.get(4)?.to_string(),
                source: tail.split_whitespace().nth(1)?.to_string(),
            })
        })
        .collect()
}

pub fn find_disk<'a>(
    mounts: &'a [Mount],
    disk: &str,
    kernel: impl Fn(&Mount) -> Option<String>,
) -> Option<&'a Mount> {
    if disk.starts_with('/') {
        return mounts
            .iter()
            .enumerate()
            .filter(|(_, mount)| holds(&mount.point, disk))
            .max_by_key(|(order, mount)| (mount.point.len(), *order))
            .map(|(_, mount)| mount);
    }
    mounts.iter().find(|mount| {
        kernel(mount).as_deref() == Some(disk) || mount.source.rsplit('/').next() == Some(disk)
    })
}

fn holds(point: &str, path: &str) -> bool {
    point == "/"
        || path == point
        || path
            .strip_prefix(point)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub fn disk_label(kernel: Option<&str>, source: &str) -> String {
    let name = kernel
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| source.rsplit('/').next().unwrap_or(source));
    match name {
        "" => "DISK".to_string(),
        name => name.to_uppercase(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub full: u64,
    pub now: u64,
    pub charging: bool,
}

pub fn read_cell(
    kind: &str,
    capacity: Option<&str>,
    energy: (Option<&str>, Option<&str>),
    status: &str,
) -> Option<Cell> {
    if kind.trim() != "Battery" {
        return None;
    }
    let number = |text: Option<&str>| text.and_then(|t| t.trim().parse::<u64>().ok());
    let charging = status.trim() == "Charging";
    match (number(energy.0), number(energy.1)) {
        (Some(full), Some(now)) if full > 0 => Some(Cell {
            full,
            now,
            charging,
        }),
        _ => number(capacity).map(|percent| Cell {
            full: 100,
            now: percent.min(100),
            charging,
        }),
    }
}

pub fn battery_rows(read: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    let number = |key: &str| read(key).and_then(|text| text.trim().parse::<f64>().ok());
    if read("type").as_deref().map(str::trim) != Some("Battery")
        || read("scope").as_deref().map(str::trim) == Some("Device")
    {
        return Vec::new();
    }
    let pair = |energy: &str, charge: &str| number(energy).or_else(|| number(charge));
    let now = pair("energy_now", "charge_now");
    let full = pair("energy_full", "charge_full");
    let design = pair("energy_full_design", "charge_full_design");
    let rate = pair("power_now", "current_now").filter(|rate| *rate > 0.0);
    let percent = match (now, full) {
        (Some(now), Some(full)) if full > 0.0 => Some((now / full * 100.0).clamp(0.0, 100.0)),
        _ => number("capacity"),
    };
    let Some(percent) = percent else {
        return Vec::new();
    };
    let status = read("status").unwrap_or_default();
    let state = match status.trim() {
        "Charging" => "charging",
        "Full" => "full",
        "Not charging" => "plugged in, not charging",
        _ => "on battery",
    };
    let mut rows = vec![(
        crate::lang::t("Charge").to_string(),
        format!("{}%, {state}", percent.round()),
    )];
    if let (Some(rate), Some(now), Some(full)) = (rate, now, full) {
        let hours = match status.trim() {
            "Charging" => Some((full - now).max(0.0) / rate),
            "Discharging" => Some(now / rate),
            _ => None,
        };
        if let Some(hours) = hours.filter(|hours| *hours < 48.0) {
            let minutes = (hours * 60.0).round() as u64;
            let label = match status.trim() {
                "Charging" => crate::lang::t("Full in"),
                _ => crate::lang::t("Time left"),
            };
            rows.push((
                label.to_string(),
                format!("{}:{:02}", minutes / 60, minutes % 60),
            ));
        }
    }
    if let (Some(full), Some(design)) = (full, design)
        && design > 0.0
    {
        let health = (full / design * 100.0).round().min(100.0);
        rows.push((crate::lang::t("Health").to_string(), format!("{health}% of new")));
    }
    if let Some(cycles) = number("cycle_count").filter(|cycles| *cycles > 0.0) {
        rows.push((crate::lang::t("Charge cycles").to_string(), format!("{cycles}")));
    }
    rows
}

pub fn read_battery_rows() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    let mut folders: Vec<std::path::PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    folders.sort();
    folders
        .iter()
        .map(|folder| battery_rows(|key| std::fs::read_to_string(folder.join(key)).ok()))
        .find(|rows| !rows.is_empty())
        .unwrap_or_default()
}

pub fn battery(cells: &[Cell]) -> Option<(&'static str, u8)> {
    let full: u64 = cells.iter().map(|cell| cell.full).sum();
    if full == 0 {
        return None;
    }
    let now: u64 = cells.iter().map(|cell| cell.now.min(cell.full)).sum();
    let percent = ((now as f64 / full as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    let label = match cells.iter().any(|cell| cell.charging) {
        true => "CHG",
        false => "BAT",
    };
    Some((label, percent))
}

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
    use super::battery_rows;

    fn supply(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(each, _)| *each == key)
                .map(|(_, value)| format!("{value}\n"))
        }
    }

    #[test]
    fn the_battery_rows_tell_charge_time_and_health() {
        let rows = battery_rows(supply(&[
            ("type", "Battery"),
            ("status", "Discharging"),
            ("energy_now", "30000000"),
            ("energy_full", "40000000"),
            ("energy_full_design", "50000000"),
            ("power_now", "10000000"),
            ("cycle_count", "212"),
        ]));
        assert_eq!(
            rows,
            [
                ("Charge".to_string(), "75%, on battery".to_string()),
                ("Time left".to_string(), "3:00".to_string()),
                ("Health".to_string(), "80% of new".to_string()),
                ("Charge cycles".to_string(), "212".to_string()),
            ]
        );
    }

    #[test]
    fn a_charging_battery_says_when_it_is_full() {
        let rows = battery_rows(supply(&[
            ("type", "Battery"),
            ("status", "Charging"),
            ("charge_now", "2000"),
            ("charge_full", "4000"),
            ("current_now", "1000"),
        ]));
        assert_eq!(rows[0].1, "50%, charging");
        assert_eq!(rows[1], ("Full in".to_string(), "2:00".to_string()));
    }

    #[test]
    fn mains_power_and_a_bare_capacity_are_handled() {
        assert!(battery_rows(supply(&[("type", "Mains"), ("online", "1")])).is_empty());
        assert!(battery_rows(supply(&[("type", "Battery"), ("scope", "Device"), ("capacity", "60")])).is_empty());
        let rows = battery_rows(supply(&[
            ("type", "Battery"),
            ("capacity", "64"),
            ("status", "Full"),
        ]));
        assert_eq!(rows, [("Charge".to_string(), "64%, full".to_string())]);
    }

    #[test]
    fn batteries_add_up_by_energy_and_charging_shows_in_the_label() {
        let one = read_cell(
            "Battery",
            Some("90"),
            (Some("50000000"), Some("25000000")),
            "Discharging",
        )
        .unwrap();
        let two = read_cell("Battery", Some("80"), (None, None), "Charging\n").unwrap();
        assert_eq!(battery(&[one]), Some(("BAT", 50)));
        assert_eq!(
            two,
            Cell {
                full: 100,
                now: 80,
                charging: true
            }
        );
        assert_eq!(battery(&[two]), Some(("CHG", 80)));
        assert_eq!(read_cell("Mains", Some("1"), (None, None), ""), None);
        assert_eq!(battery(&[]), None);
        assert_eq!(Metric::parse("battery"), Ok(Metric::Battery));
    }

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

    fn font() -> Font {
        Font::find(None, 13.0).expect("this machine has no usable font")
    }

    fn fields(cpu: u8, ram: u8, disk: u8) -> [(&'static str, u8); 3] {
        [("CPU", cpu), ("RAM", ram), ("VDA1", disk)]
    }

    fn columns(image: &Image, from: u32, to: u32) -> Vec<u8> {
        (0..image.height)
            .flat_map(|y| {
                let row = (y * image.width) as usize * 4;
                image.rgba[row + from as usize * 4..row + to as usize * 4].to_vec()
            })
            .collect()
    }

    #[test]
    fn the_width_never_changes_with_the_numbers() {
        let font = font();
        let white = [255, 255, 255];
        let low = paint(&font, &fields(1, 2, 3), white);
        let mid = paint(&font, &fields(9, 10, 99), white);
        let high = paint(&font, &fields(100, 100, 100), white);
        assert_eq!(low.width, high.width);
        assert_eq!(mid.width, high.width);
    }

    #[test]
    fn a_second_digit_moves_nothing_but_the_digits() {
        let font = font();
        let white = [255, 255, 255];
        let nine = paint(&font, &fields(9, 42, 15), white);
        let ten = paint(&font, &fields(10, 42, 15), white);
        let cpu = font.width_of("CPU").floor() as u32;
        let slot_end = font.width_of("CPU") + font.width_of(" ") + font.width_of("100%");
        let percent = (slot_end - font.width_of("%")).round() as u32;
        assert_eq!(
            columns(&nine, 0, cpu),
            columns(&ten, 0, cpu),
            "the CPU label"
        );
        assert_eq!(
            columns(&nine, percent, nine.width),
            columns(&ten, percent, ten.width),
            "the percent sign and everything after it"
        );
        assert_ne!(nine.rgba, ten.rgba, "the digits did change");
    }

    #[test]
    fn a_longer_disk_name_only_widens_its_own_label() {
        let font = font();
        let white = [255, 255, 255];
        let short = paint(&font, &[("CPU", 5), ("SDA3", 5)], white);
        let long = paint(&font, &[("CPU", 5), ("NVME0N1P3", 5)], white);
        assert!(long.width > short.width);
    }

    const MOUNTINFO: &str = "22 1 0:21 / /proc rw - proc proc rw\n\
        29 1 254:1 / / rw,relatime shared:1 - ext4 /dev/vda1 rw\n\
        31 29 8:3 / /home rw - ext4 /dev/sda3 rw\n\
        32 29 0:35 / /data rw - btrfs /dev/nvme0n1p2 rw\n";

    fn kernel(mount: &Mount) -> Option<String> {
        (mount.device == "254:1").then(|| "vda1".to_string())
    }

    #[test]
    fn mountinfo_lines_become_mounts() {
        let all = mounts(MOUNTINFO);
        assert_eq!(all.len(), 4);
        assert_eq!(
            all[1],
            Mount {
                device: "254:1".into(),
                point: "/".into(),
                source: "/dev/vda1".into()
            }
        );
    }

    #[test]
    fn a_mount_point_finds_its_own_mount() {
        let all = mounts(MOUNTINFO);
        assert_eq!(find_disk(&all, "/", kernel).unwrap().source, "/dev/vda1");
        assert_eq!(
            find_disk(&all, "/home", kernel).unwrap().source,
            "/dev/sda3"
        );
    }

    #[test]
    fn a_folder_finds_the_mount_that_holds_it() {
        let all = mounts(MOUNTINFO);
        assert_eq!(find_disk(&all, "/home/dh", kernel).unwrap().point, "/home");
        assert_eq!(find_disk(&all, "/homework", kernel).unwrap().point, "/");
    }

    #[test]
    fn a_later_mount_on_the_same_point_hides_an_earlier_one() {
        let all = mounts(
            "1 0 8:3 / / rw - ext4 /dev/sda3 rw\n2 1 0:35 / / rw - btrfs /dev/nvme0n1p3 rw\n",
        );
        assert_eq!(
            find_disk(&all, "/", kernel).unwrap().source,
            "/dev/nvme0n1p3"
        );
    }

    #[test]
    fn a_device_name_finds_its_mount() {
        let all = mounts(MOUNTINFO);
        assert_eq!(find_disk(&all, "sda3", kernel).unwrap().point, "/home");
        assert_eq!(find_disk(&all, "vda1", kernel).unwrap().point, "/");
        assert_eq!(find_disk(&all, "nvme0n1p2", kernel).unwrap().point, "/data");
    }

    #[test]
    fn a_disk_that_is_not_mounted_finds_nothing() {
        let all = mounts(MOUNTINFO);
        assert_eq!(find_disk(&all, "sdb1", kernel), None);
    }

    #[test]
    fn metrics_parse_from_the_config_words() {
        assert_eq!(Metric::parse("cpu"), Ok(Metric::Cpu));
        assert_eq!(Metric::parse("ram"), Ok(Metric::Ram));
        assert_eq!(Metric::parse("/home"), Ok(Metric::Disk("/home".into())));
        assert_eq!(Metric::parse("sda3"), Ok(Metric::Disk("sda3".into())));
        assert_eq!(Metric::parse("/dev/sda3"), Ok(Metric::Disk("sda3".into())));
        assert!(Metric::parse(" ").is_err());
    }

    #[test]
    fn the_disk_is_named_by_its_kernel_name_in_capitals() {
        assert_eq!(disk_label(Some("sda3"), "/dev/sda3"), "SDA3");
        assert_eq!(disk_label(Some("vda1"), "/dev/root"), "VDA1");
    }

    #[test]
    fn a_disk_with_no_kernel_name_uses_its_mount_source() {
        assert_eq!(disk_label(None, "/dev/nvme0n1p3"), "NVME0N1P3");
        assert_eq!(disk_label(None, ""), "DISK");
    }
}
