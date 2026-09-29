use crate::image::Image;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    pub cpu: u8,
    pub ram: u8,
    pub disk: u8,
}

pub fn paint(font: &Font, stats: &Stats, label: [u8; 3], value: [u8; 3]) -> Image {
    let slot = font.width_of("100%").ceil() as u32;
    let space = font.width_of(" ").ceil() as u32;
    let group = font.width_of("   ").ceil() as u32;
    let mut pieces: Vec<(u32, Image)> = Vec::new();
    let mut pen = 0;
    for (at, (name, share)) in [("CPU", stats.cpu), ("RAM", stats.ram), ("DSK", stats.disk)]
        .into_iter()
        .enumerate()
    {
        if at > 0 {
            pen += group;
        }
        let name = font.paint(name, label);
        let width = name.width;
        pieces.push((pen, name));
        pen += width + space;
        let number = font.paint(&format!("{share}%"), value);
        let width = number.width;
        pieces.push((pen + slot.saturating_sub(width), number));
        pen += slot.max(width);
    }

    let height = pieces
        .iter()
        .map(|(_, piece)| piece.height)
        .max()
        .unwrap_or(1);
    let mut image = Image {
        width: pen.max(1),
        height,
        rgba: vec![0; (pen.max(1) as usize) * (height as usize) * 4],
    };
    for (left, piece) in &pieces {
        for y in 0..piece.height as usize {
            let from = y * piece.width as usize * 4;
            let to = (y * image.width as usize + *left as usize) * 4;
            let run = (piece.width as usize * 4).min(image.rgba.len() - to);
            image.rgba[to..to + run].copy_from_slice(&piece.rgba[from..from + run]);
        }
    }
    image
}

pub fn cpu_times(proc_stat: &str) -> Option<CpuTimes> {
    let line = proc_stat.lines().find(|line| line.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .map(|field| field.parse().ok())
        .collect::<Option<_>>()?;
    if fields.len() < 4 {
        return None;
    }
    let total: u64 = fields.iter().take(8).sum();
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
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
    };
    let total = field("MemTotal")?;
    let available = field("MemAvailable")?;
    Some(percent(total.saturating_sub(available), total))
}

pub fn disk_percent(total: u64, free: u64, available: u64) -> u8 {
    let used = total.saturating_sub(free);
    percent(used, used + available)
}

fn percent(part: u64, whole: u64) -> u8 {
    if whole == 0 {
        return 0;
    }
    let share = part.min(whole) as f64 / whole as f64;
    (share * 100.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 5 50 800 20 3 2 0 0 0\n\
                        cpu0 50 2 25 400 10 1 1 0 0 0\n\
                        intr 12345\n";

    #[test]
    fn the_cpu_line_splits_into_busy_and_total() {
        let times = cpu_times(STAT).unwrap();
        assert_eq!(times.total, 980);
        assert_eq!(times.busy, 160);
    }

    #[test]
    fn a_per_core_line_is_not_the_total() {
        let only_core = "cpu0 50 2 25 400 10 1 1 0\n";
        assert_eq!(cpu_times(only_core), None);
    }

    #[test]
    fn cpu_use_is_the_busy_share_between_two_readings() {
        let before = CpuTimes {
            busy: 100,
            total: 1000,
        };
        let after = CpuTimes {
            busy: 150,
            total: 1200,
        };
        assert_eq!(cpu_percent(before, after), 25);
    }

    #[test]
    fn no_time_passing_reads_as_zero_not_a_panic() {
        let same = CpuTimes { busy: 5, total: 10 };
        assert_eq!(cpu_percent(same, same), 0);
    }

    #[test]
    fn ram_use_counts_what_is_not_available() {
        let meminfo = "MemTotal:       16000000 kB\n\
                       MemFree:         2000000 kB\n\
                       MemAvailable:   12000000 kB\n";
        assert_eq!(ram_percent(meminfo), Some(25));
    }

    #[test]
    fn ram_without_available_is_unknown() {
        assert_eq!(ram_percent("MemTotal: 100 kB\n"), None);
    }

    #[test]
    fn disk_use_matches_df_and_leaves_out_the_root_reserve() {
        assert_eq!(disk_percent(1000, 400, 350), 63);
        assert_eq!(disk_percent(1000, 1000, 1000), 0);
        assert_eq!(disk_percent(0, 0, 0), 0);
    }

    fn red_and_white(image: &Image) -> (usize, usize) {
        let pixels = image.rgba.as_chunks::<4>().0;
        let solid = pixels.iter().filter(|pixel| pixel[3] == 255);
        solid.fold((0, 0), |(red, white), pixel| {
            if pixel[0] > 200 && pixel[1] < 60 {
                (red + 1, white)
            } else if pixel[0] > 200 && pixel[1] > 200 {
                (red, white + 1)
            } else {
                (red, white)
            }
        })
    }

    #[test]
    fn the_labels_are_red_and_the_numbers_white() {
        let Ok(font) = Font::find(None, 16.0) else {
            return;
        };
        let stats = Stats {
            cpu: 7,
            ram: 42,
            disk: 63,
        };
        let image = paint(&font, &stats, crate::RED, [236, 238, 244]);
        let (red, white) = red_and_white(&image);
        assert!(red > 0 && white > 0, "red {red} white {white}");
    }

    #[test]
    fn the_width_holds_still_as_the_numbers_change() {
        let Ok(font) = Font::find(None, 16.0) else {
            return;
        };
        let low = Stats {
            cpu: 1,
            ram: 2,
            disk: 3,
        };
        let high = Stats {
            cpu: 100,
            ram: 99,
            disk: 88,
        };
        let ink = [236, 238, 244];
        assert_eq!(
            paint(&font, &low, crate::RED, ink).width,
            paint(&font, &high, crate::RED, ink).width
        );
    }
}
