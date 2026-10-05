#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plugged {
    pub usage: String,
    pub auto: bool,
    pub ignore: bool,
    pub system: bool,
    pub mounted: bool,
}

impl Plugged {
    pub fn wants_mount(&self) -> bool {
        self.usage == "filesystem" && self.auto && !self.ignore && !self.system && !self.mounted
    }
}

pub fn label(id_label: &str, size: u64, device: &str) -> String {
    let id_label = id_label.trim();
    if !id_label.is_empty() {
        return id_label.to_string();
    }
    if size > 0 {
        return format!("{} disk", size_words(size));
    }
    device.rsplit('/').next().unwrap_or(device).to_string()
}

pub fn size_words(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut size = bytes as f64 / 1000.0;
    let mut unit = 0;
    while size >= 1000.0 && unit + 1 < UNITS.len() {
        size /= 1000.0;
        unit += 1;
    }
    match size < 10.0 {
        true => format!("{size:.1} {}", UNITS[unit]),
        false => format!("{} {}", size.round(), UNITS[unit]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stick() -> Plugged {
        Plugged {
            usage: "filesystem".into(),
            auto: true,
            ignore: false,
            system: false,
            mounted: false,
        }
    }

    #[test]
    fn a_fresh_usb_stick_is_mounted() {
        assert!(stick().wants_mount());
    }

    #[test]
    fn system_hidden_mounted_and_raw_disks_are_left_alone() {
        for plugged in [
            Plugged { system: true, ..stick() },
            Plugged { ignore: true, ..stick() },
            Plugged { mounted: true, ..stick() },
            Plugged { auto: false, ..stick() },
            Plugged { usage: "crypto".into(), ..stick() },
            Plugged { usage: "".into(), ..stick() },
        ] {
            assert!(!plugged.wants_mount(), "{plugged:?}");
        }
    }

    #[test]
    fn sizes_read_like_the_box_says() {
        assert_eq!(size_words(16_000_000_000), "16 GB");
        assert_eq!(size_words(2_000_000_000_000), "2.0 TB");
        assert_eq!(size_words(512_000_000), "512 MB");
    }

    #[test]
    fn a_disk_is_named_by_its_label_then_its_size() {
        assert_eq!(label("KINGSTON ", 0, "/dev/sdb1"), "KINGSTON");
        assert_eq!(label("", 16_000_000_000, "/dev/sdb1"), "16 GB disk");
        assert_eq!(label("", 0, "/dev/sdb1"), "sdb1");
    }
}
