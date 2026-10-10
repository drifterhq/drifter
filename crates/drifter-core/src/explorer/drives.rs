use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::prefs::on_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub device: Option<String>,
    pub disk: Option<String>,
    pub name: String,
    pub mount: Option<PathBuf>,
    pub size: u64,
    pub free: Option<u64>,
    pub removable: bool,
}

impl Drive {
    pub fn share_used(&self) -> Option<f64> {
        let free = self.free?;
        (self.size > 0).then(|| (1.0 - free as f64 / self.size as f64).clamp(0.0, 1.0))
    }

    pub fn ejectable(&self) -> bool {
        self.device.is_some() && (self.removable || self.mount.is_some())
    }
}

const SYSTEM: [&str; 9] = ["/", "/boot", "/boot/efi", "/boot/firmware", "/efi", "/home", "/usr", "/var", "[SWAP]"];
const SHARED: [&str; 3] = ["/media/", "/run/media/", "/mnt/"];
const NETWORK: [&str; 8] = ["nfs", "nfs4", "cifs", "smb3", "fuse.sshfs", "fuse.rclone", "davfs", "9p"];

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\'
            && bytes.get(at + 1) == Some(&b'x')
            && let Some(byte) = text.get(at + 2..at + 4).and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            at += 4;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn pairs(line: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut rest = line.trim();
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].trim().to_string();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.insert(key, unescape(&after[..end]));
        rest = &after[end + 1..];
    }
    out
}

pub const COLUMNS: &str = "NAME,PATH,PKNAME,LABEL,SIZE,FSTYPE,MOUNTPOINT,RM,HOTPLUG,TYPE,FSAVAIL,FSSIZE";

pub fn from_lsblk(text: &str) -> Vec<Drive> {
    let rows: Vec<HashMap<String, String>> = text.lines().map(pairs).filter(|row| !row.is_empty()).collect();
    let parent: HashMap<&str, &str> = rows.iter().filter_map(|row| Some((row.get("NAME")?.as_str(), row.get("PKNAME")?.as_str()))).filter(|(_, up)| !up.is_empty()).collect();
    let top = |name: &str| {
        let mut at = name;
        let mut steps = 0;
        while let Some(up) = parent.get(at) {
            at = up;
            steps += 1;
            if steps > 8 {
                break;
            }
        }
        at.to_string()
    };
    let system: HashSet<String> = rows
        .iter()
        .filter(|row| row.get("MOUNTPOINT").is_some_and(|point| SYSTEM.contains(&point.as_str())))
        .filter_map(|row| row.get("NAME").map(|name| top(name)))
        .collect();
    let mut out = Vec::new();
    for row in &rows {
        let get = |key: &str| row.get(key).map(String::as_str).unwrap_or("");
        let fstype = get("FSTYPE");
        let kind = get("TYPE");
        let point = get("MOUNTPOINT");
        let shared = SHARED.iter().any(|root| point.starts_with(root));
        let usable = !fstype.is_empty() && !["swap", "crypto_LUKS", "LVM2_member", "linux_raid_member", "squashfs"].contains(&fstype) && ["part", "disk", "crypt", "lvm", "md", "rom"].contains(&kind);
        if !(usable && !system.contains(&top(get("NAME")))) && !shared {
            continue;
        }
        if SYSTEM.contains(&point) {
            continue;
        }
        let size = get("FSSIZE").parse().ok().filter(|size: &u64| *size > 0).unwrap_or_else(|| get("SIZE").parse().unwrap_or(0));
        let disk = top(get("NAME"));
        out.push(Drive {
            device: Some(get("PATH").to_string()).filter(|path| !path.is_empty()),
            disk: rows.iter().find(|each| each.get("NAME").map(String::as_str) == Some(disk.as_str())).and_then(|each| each.get("PATH").cloned()),
            name: crate::disks::label(get("LABEL"), size, get("PATH")),
            mount: (!point.is_empty()).then(|| PathBuf::from(point)),
            size,
            free: get("FSAVAIL").parse().ok(),
            removable: get("RM") == "1" || get("HOTPLUG") == "1",
        });
    }
    out
}

pub fn from_mounts(mounts: &str, known: &[Drive]) -> Vec<Drive> {
    let mut out = Vec::new();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_), Some(point), Some(kind)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let point = PathBuf::from(point.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\"));
        let shared = SHARED.iter().any(|root| point.to_string_lossy().starts_with(root));
        if !(NETWORK.contains(&kind) || shared) {
            continue;
        }
        let seen = known.iter().chain(out.iter()).any(|drive: &Drive| drive.mount.as_ref() == Some(&point));
        if !seen {
            out.push(Drive { device: None, disk: None, name: super::entry::name_of(&point), mount: Some(point), size: 0, free: None, removable: false });
        }
    }
    out
}

pub fn computer() -> Drive {
    Drive { device: None, disk: None, name: crate::lang::t("Computer").to_string(), mount: Some(PathBuf::from("/")), size: 0, free: None, removable: false }
}

pub fn list() -> Vec<Drive> {
    let mut found = match on_path("lsblk") {
        true => Command::new("lsblk")
            .args(["-P", "-b", "-o", COLUMNS])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|out| from_lsblk(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or_default(),
        false => Vec::new(),
    };
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let more = from_mounts(&mounts, &found);
    found.extend(more);
    found.insert(0, computer());
    for drive in found.iter_mut().filter(|drive| drive.device.is_none()) {
        if let Some((free, total)) = drive.mount.as_deref().and_then(super::entry::free_space) {
            drive.free = Some(free);
            drive.size = total;
        }
    }
    found
}

fn udisks(args: &[&str]) -> Result<String, String> {
    if !on_path("udisksctl") {
        return Err(crate::lang::t("Install udisks2 to mount and eject disks").to_string());
    }
    let out = Command::new("udisksctl").args(args).stdin(Stdio::null()).output().map_err(|error| error.to_string())?;
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    match out.status.success() {
        true => Ok(said),
        false => {
            let words = String::from_utf8_lossy(&out.stderr);
            let last = words.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
            let last = last.rsplit_once(": ").map_or(last, |(_, tail)| tail);
            Err(match last.is_empty() {
                true => out.status.to_string(),
                false => last.to_string(),
            })
        }
    }
}

pub fn mounted_at(said: &str) -> Option<PathBuf> {
    let (_, tail) = said.trim().split_once(" at ")?;
    Some(PathBuf::from(tail.trim().trim_end_matches('.')))
}

pub fn mount(device: &str) -> Result<PathBuf, String> {
    let said = udisks(&["mount", "--no-user-interaction", "-b", device])?;
    mounted_at(&said).or_else(|| list().into_iter().find(|drive| drive.device.as_deref() == Some(device)).and_then(|drive| drive.mount)).ok_or_else(|| said.trim().to_string())
}

pub fn unmount(device: &str) -> Result<(), String> {
    udisks(&["unmount", "--no-user-interaction", "-b", device]).map(|_| ())
}

pub fn eject(device: &str, disk: Option<&str>, mounted: bool) -> Result<(), String> {
    if mounted {
        unmount(device)?;
    }
    match disk {
        Some(disk) => udisks(&["power-off", "--no-user-interaction", "-b", disk]).map(|_| ()),
        None => Ok(()),
    }
}

pub fn watch_dirs() -> Vec<PathBuf> {
    ["/dev/disk/by-id", "/dev/disk/by-uuid"].iter().map(PathBuf::from).filter(|path| Path::new(path).is_dir()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PI: &str = r#"NAME="loop0" PATH="/dev/loop0" PKNAME="" LABEL="origin:rpi-swap" SIZE="2147483648" FSTYPE="swap" MOUNTPOINT="" RM="0" HOTPLUG="0" TYPE="loop" FSAVAIL="" FSSIZE=""
NAME="mmcblk0" PATH="/dev/mmcblk0" PKNAME="" LABEL="" SIZE="127865454592" FSTYPE="" MOUNTPOINT="" RM="0" HOTPLUG="1" TYPE="disk" FSAVAIL="" FSSIZE=""
NAME="mmcblk0p1" PATH="/dev/mmcblk0p1" PKNAME="mmcblk0" LABEL="bootfs" SIZE="536870912" FSTYPE="vfat" MOUNTPOINT="/boot/firmware" RM="0" HOTPLUG="1" TYPE="part" FSAVAIL="450144256" FSSIZE="534763520"
NAME="mmcblk0p2" PATH="/dev/mmcblk0p2" PKNAME="mmcblk0" LABEL="rootfs" SIZE="127320195072" FSTYPE="ext4" MOUNTPOINT="/" RM="0" HOTPLUG="1" TYPE="part" FSAVAIL="98007379968" FSSIZE="125000000000"
NAME="sda" PATH="/dev/sda" PKNAME="" LABEL="" SIZE="32000000000" FSTYPE="" MOUNTPOINT="" RM="1" HOTPLUG="1" TYPE="disk" FSAVAIL="" FSSIZE=""
NAME="sda1" PATH="/dev/sda1" PKNAME="sda" LABEL="My\x20Stick" SIZE="32000000000" FSTYPE="exfat" MOUNTPOINT="/media/dh/My Stick" RM="1" HOTPLUG="1" TYPE="part" FSAVAIL="8000000000" FSSIZE="32000000000"
NAME="sdb" PATH="/dev/sdb" PKNAME="" LABEL="" SIZE="1000000000000" FSTYPE="" MOUNTPOINT="" RM="0" HOTPLUG="0" TYPE="disk" FSAVAIL="" FSSIZE=""
NAME="sdb1" PATH="/dev/sdb1" PKNAME="sdb" LABEL="" SIZE="1000000000000" FSTYPE="ntfs" MOUNTPOINT="" RM="0" HOTPLUG="0" TYPE="part" FSAVAIL="" FSSIZE=""
NAME="zram0" PATH="/dev/zram0" PKNAME="" LABEL="" SIZE="4000000000" FSTYPE="" MOUNTPOINT="[SWAP]" RM="0" HOTPLUG="0" TYPE="disk" FSAVAIL="" FSSIZE=""
"#;

    #[test]
    fn system_disks_stay_out_and_others_show() {
        let found = from_lsblk(PI);
        assert_eq!(found.len(), 2, "{found:#?}");
        let stick = &found[0];
        assert_eq!(stick.name, "My Stick");
        assert_eq!(stick.mount, Some(PathBuf::from("/media/dh/My Stick")));
        assert_eq!(stick.disk.as_deref(), Some("/dev/sda"));
        assert!(stick.removable && stick.ejectable());
        assert_eq!(stick.share_used(), Some(0.75));
        let inside = &found[1];
        assert_eq!(inside.device.as_deref(), Some("/dev/sdb1"));
        assert_eq!(inside.mount, None);
        assert_eq!(inside.name, "1.0 TB disk");
        assert!(!inside.removable);
    }

    #[test]
    fn network_mounts_join_the_list_once() {
        let known = from_lsblk(PI);
        let mounts = "server:/share /mnt/share nfs4 rw 0 0\n/dev/sda1 /media/dh/My\\040Stick exfat rw 0 0\nproc /proc proc rw 0 0\ngvfsd-fuse /run/user/1000/gvfs fuse.gvfsd-fuse rw 0 0\n";
        let more = from_mounts(mounts, &known);
        assert_eq!(more.len(), 1);
        assert_eq!(more[0].mount, Some(PathBuf::from("/mnt/share")));
        assert_eq!(more[0].name, "share");
        assert!(!more[0].ejectable());
    }

    #[test]
    fn udisks_words_give_the_mount_point() {
        assert_eq!(mounted_at("Mounted /dev/sda1 at /media/dh/My Stick\n"), Some(PathBuf::from("/media/dh/My Stick")));
        assert_eq!(mounted_at("Mounted /dev/sda1 at /media/dh/STICK.\n"), Some(PathBuf::from("/media/dh/STICK")));
        assert_eq!(mounted_at("nothing"), None);
    }
}
