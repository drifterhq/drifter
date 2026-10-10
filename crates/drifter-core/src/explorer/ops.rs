use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Progress {
    pub bytes: AtomicU64,
    pub total: AtomicU64,
    pub items: AtomicU64,
    pub count: AtomicU64,
    pub counting: AtomicBool,
    pub stop: AtomicBool,
}

impl Progress {
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn share(&self) -> Option<f64> {
        if self.counting.load(Ordering::Relaxed) {
            return None;
        }
        let (bytes, total) = (self.bytes.load(Ordering::Relaxed), self.total.load(Ordering::Relaxed));
        let (items, count) = (self.items.load(Ordering::Relaxed), self.count.load(Ordering::Relaxed));
        match (total, count) {
            (1.., _) => Some((bytes as f64 / total as f64).min(1.0)),
            (0, 1..) => Some((items as f64 / count as f64).min(1.0)),
            _ => None,
        }
    }

    fn wrote(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    fn item(&self) {
        self.items.fetch_add(1, Ordering::Relaxed);
    }

    fn check(&self) -> io::Result<()> {
        match self.stopped() {
            true => Err(io::Error::new(io::ErrorKind::Interrupted, crate::lang::t("Stopped"))),
            false => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    pub from: PathBuf,
    pub file: PathBuf,
    pub info: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clash {
    #[default]
    KeepBoth,
    Replace,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Copy { from: Vec<PathBuf>, into: PathBuf, clash: Clash },
    Move { from: Vec<PathBuf>, into: PathBuf, clash: Clash },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
    Restore(Vec<PathBuf>),
    EmptyTrash,
    Undo(Undo),
    Compress(Vec<PathBuf>),
    Extract(Vec<PathBuf>),
    Mount { device: String, name: String },
    Unmount { device: String, name: String },
    Eject { device: String, disk: Option<String>, name: String, mounted: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    Made(Vec<PathBuf>),
    Moved(Vec<(PathBuf, PathBuf)>),
    Renamed(PathBuf, PathBuf),
    RenamedAll(Vec<(PathBuf, PathBuf)>),
    Trashed(Vec<Trashed>),
    Then(Box<Undo>, Vec<Trashed>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub id: u64,
    pub stopped: bool,
    pub undo: Option<Undo>,
    pub made: Vec<PathBuf>,
    pub failed: Vec<String>,
    pub words: String,
    pub go: Option<PathBuf>,
}

impl Job {
    pub fn count(&self) -> usize {
        match self {
            Job::Copy { from, .. } | Job::Move { from, .. } => from.len(),
            Job::Trash(paths) | Job::Delete(paths) | Job::Restore(paths) | Job::Compress(paths) | Job::Extract(paths) => paths.len(),
            Job::EmptyTrash => 0,
            Job::Undo(_) | Job::Mount { .. } | Job::Unmount { .. } | Job::Eject { .. } => 1,
        }
    }

    pub fn doing(&self) -> String {
        use crate::explorer::entry::{items, name_of};
        let what = match self {
            Job::Copy { from, .. } | Job::Move { from, .. } | Job::Trash(from) | Job::Delete(from) | Job::Restore(from) | Job::Compress(from) | Job::Extract(from) => match from.as_slice() {
                [one] => name_of(one),
                many => items(many.len()),
            },
            Job::Mount { name, .. } | Job::Unmount { name, .. } | Job::Eject { name, .. } => name.clone(),
            Job::EmptyTrash | Job::Undo(_) => String::new(),
        };
        match self {
            Job::Copy { into, .. } => crate::lang::tf("Copying {} to {}", &[&what, &name_of(into)]),
            Job::Move { into, .. } => crate::lang::tf("Moving {} to {}", &[&what, &name_of(into)]),
            Job::Trash(_) => crate::lang::tf("Moving {} to the Trash", &[&what]),
            Job::Delete(_) => crate::lang::tf("Deleting {}", &[&what]),
            Job::Restore(_) => crate::lang::tf("Putting back {}", &[&what]),
            Job::Compress(_) => crate::lang::tf("Compressing {}", &[&what]),
            Job::Extract(_) => crate::lang::tf("Extracting {}", &[&what]),
            Job::Mount { .. } => crate::lang::tf("Opening {}", &[&what]),
            Job::Unmount { .. } => crate::lang::tf("Unmounting {}", &[&what]),
            Job::Eject { .. } => crate::lang::tf("Ejecting {}", &[&what]),
            Job::EmptyTrash => crate::lang::t("Emptying the Trash").to_string(),
            Job::Undo(_) => crate::lang::t("Undoing").to_string(),
        }
    }
}

pub fn trash_root() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    data.join("Trash")
}

pub fn trash_files() -> PathBuf {
    trash_root().join("files")
}

pub fn in_trash(path: &Path) -> bool {
    path.starts_with(trash_files()) || root_of(path).is_some()
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn top_of_root(root: &Path) -> Option<PathBuf> {
    let name = root.file_name()?.to_string_lossy().into_owned();
    if name == format!(".Trash-{}", uid()) {
        return root.parent().map(Path::to_path_buf);
    }
    let shared = root.parent()?;
    (name == uid().to_string() && shared.file_name()? == ".Trash").then(|| shared.parent().map(Path::to_path_buf)).flatten()
}

fn top_real(root: &Path) -> Option<PathBuf> {
    let top = top_of_root(root)?;
    Some(top.canonicalize().unwrap_or(top))
}

pub fn root_of(path: &Path) -> Option<PathBuf> {
    let home = trash_root();
    path.ancestors().filter(|at| at.file_name().is_some_and(|name| name == "files")).filter_map(Path::parent).find(|root| *root == home || top_of_root(root).is_some()).map(Path::to_path_buf)
}

fn device(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).ok().map(|meta| meta.dev())
}

fn real(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

fn mount_points() -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string("/proc/self/mounts").ok()?;
    Some(text.lines().filter_map(|line| line.split_whitespace().nth(1)).map(|point| PathBuf::from(point.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\"))).collect())
}

fn top_of(path: &Path) -> Option<PathBuf> {
    let path = real(path)?;
    if let Some(points) = mount_points() {
        return points.into_iter().filter(|point| path.starts_with(point) && path != *point).max_by_key(|point| point.components().count());
    }
    let dev = device(&path)?;
    let mut top = None;
    for at in path.ancestors().skip(1) {
        match device(at) {
            Some(here) if here == dev => top = Some(at.to_path_buf()),
            _ => break,
        }
    }
    top
}

fn usable(root: &Path) -> bool {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let made = std::fs::DirBuilder::new().mode(0o700).create(root);
    if made.as_ref().is_err_and(|error| error.kind() != io::ErrorKind::AlreadyExists) {
        return false;
    }
    std::fs::symlink_metadata(root).is_ok_and(|meta| meta.is_dir() && meta.uid() == uid())
}

fn trusted(root: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mine = |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir() && meta.uid() == uid());
    let shared_ok = match root.parent() {
        Some(shared) if shared.file_name().is_some_and(|name| name == ".Trash") => std::fs::symlink_metadata(shared).is_ok_and(|meta| meta.is_dir() && meta.permissions().mode() & 0o1000 != 0),
        _ => true,
    };
    shared_ok && mine(root) && mine(&root.join("files"))
}

pub fn trash_for(home: &Path, path: &Path) -> PathBuf {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let home_dev = home.ancestors().find_map(device);
    let (Some(dev), Some(home_dev)) = (device(path), home_dev) else {
        return home.to_path_buf();
    };
    if dev == home_dev {
        return home.to_path_buf();
    }
    let Some(top) = top_of(path) else {
        return home.to_path_buf();
    };
    let shared = top.join(".Trash");
    let sticky = std::fs::symlink_metadata(&shared).is_ok_and(|meta| meta.is_dir() && meta.permissions().mode() & 0o1000 != 0 && meta.dev() == dev);
    if sticky {
        let mine = shared.join(uid().to_string());
        if usable(&mine) {
            return mine;
        }
    }
    let own = top.join(format!(".Trash-{}", uid()));
    match usable(&own) {
        true => own,
        false => home.to_path_buf(),
    }
}

pub fn other_trashes() -> Vec<PathBuf> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mut out: Vec<PathBuf> = Vec::new();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_), Some(point), Some(kind)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if ["proc", "sysfs", "devtmpfs", "devpts", "cgroup", "cgroup2", "securityfs", "pstore", "bpf", "debugfs", "tracefs", "mqueue", "hugetlbfs", "configfs", "fusectl", "efivarfs", "autofs", "binfmt_misc"].contains(&kind) {
            continue;
        }
        let top = PathBuf::from(point.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\"));
        for root in [top.join(format!(".Trash-{}", uid())), top.join(".Trash").join(uid().to_string())] {
            if trusted(&root) && !out.contains(&root) {
                out.push(root);
            }
        }
    }
    let home = trash_root();
    out.retain(|root| *root != home);
    out
}

pub fn unique(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if std::fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = split_name(name);
    (2..)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|path| std::fs::symlink_metadata(path).is_err())
        .unwrap_or(first)
}

pub fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(at) if at > 0 => (&name[..at], &name[at..]),
        _ => (name, ""),
    }
}

pub fn copy_into(from: &Path, into: &Path) -> io::Result<PathBuf> {
    copy_into_with(from, into, &Progress::default())
}

pub fn copy_into_with(from: &Path, into: &Path, progress: &Progress) -> io::Result<PathBuf> {
    let name = from.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    if into.starts_with(from) {
        return Err(io::Error::other(crate::lang::t("A folder cannot go inside itself")));
    }
    let target = match from.parent() == Some(into) {
        true => {
            let (stem, ext) = split_name(&name);
            unique(into, &format!("{stem} {}{ext}", crate::lang::t("copy")))
        }
        false => unique(into, &name),
    };
    copy_onto(from, &target, progress)?;
    Ok(target)
}

fn copy_onto(from: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    if let Err(error) = copy_tree(from, target, progress) {
        if error.kind() != io::ErrorKind::AlreadyExists {
            let _ = remove(target);
        }
        return Err(error);
    }
    Ok(())
}

pub fn folder_size(path: &Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut count = 0;
    let mut waiting = vec![path.to_path_buf()];
    while let Some(dir) = waiting.pop() {
        let Ok(items) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in items.flatten() {
            let Ok(meta) = item.path().symlink_metadata() else {
                continue;
            };
            count += 1;
            match meta.is_dir() {
                true => waiting.push(item.path()),
                false => bytes += meta.len(),
            }
        }
    }
    (bytes, count)
}

pub fn clashes(from: &[PathBuf], into: &Path) -> Vec<PathBuf> {
    from.iter().filter(|path| path.parent() != Some(into)).filter_map(|path| Some(into.join(path.file_name()?))).filter(|plain| std::fs::symlink_metadata(plain).is_ok()).collect()
}

enum Placed {
    Free,
    Skip,
    Over(PathBuf, Trashed),
}

struct Spot<'a> {
    root: &'a Path,
    into: &'a Path,
    clash: Clash,
    made: &'a [PathBuf],
    measured: bool,
}

fn place(spot: &Spot, from: &Path, progress: &Progress) -> io::Result<Placed> {
    let (root, into) = (spot.root, spot.into);
    if spot.clash == Clash::KeepBoth || from.parent() == Some(into) {
        return Ok(Placed::Free);
    }
    let plain = into.join(from.file_name().ok_or_else(|| io::Error::other("no name"))?);
    if std::fs::symlink_metadata(&plain).is_err() || spot.made.contains(&plain) {
        return Ok(Placed::Free);
    }
    if spot.clash == Clash::Skip {
        if spot.measured {
            let skipped = Progress::default();
            let _ = measure(from, &skipped);
            let less = |have: &std::sync::atomic::AtomicU64, by: u64| {
                let _ = have.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |now| Some(now.saturating_sub(by)));
            };
            less(&progress.total, skipped.total.load(Ordering::Relaxed));
            less(&progress.count, skipped.count.load(Ordering::Relaxed));
        }
        return Ok(Placed::Skip);
    }
    if into.starts_with(from) {
        return Err(io::Error::other(crate::lang::t("A folder cannot go inside itself")));
    }
    if from.starts_with(&plain) {
        return Err(io::Error::other(crate::lang::t("A folder cannot be replaced by something inside it")));
    }
    let old = trash_in_with(&trash_for(root, &plain), &plain, progress)?;
    Ok(Placed::Over(plain, old))
}

fn with_old(undo: Option<Undo>, replaced: Vec<Trashed>) -> Option<Undo> {
    match (undo, replaced.is_empty()) {
        (undo, true) => undo,
        (Some(undo), false) => Some(Undo::Then(Box::new(undo), replaced)),
        (None, false) => Some(Undo::Trashed(replaced)),
    }
}

fn copy_tree(from: &Path, to: &Path, progress: &Progress) -> io::Result<()> {
    progress.check()?;
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)?;
        progress.item();
        return Ok(());
    }
    if meta.is_dir() {
        std::fs::create_dir(to)?;
        progress.item();
        for item in std::fs::read_dir(from)? {
            let item = item?;
            copy_tree(&item.path(), &to.join(item.file_name()), progress)?;
        }
        return std::fs::set_permissions(to, meta.permissions());
    }
    if !meta.is_file() {
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};
        if meta.file_type().is_fifo() {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::CString::new(to.as_os_str().as_bytes()).map_err(io::Error::other)?;
            if unsafe { libc::mkfifo(name.as_ptr(), (meta.permissions().mode() & 0o7777) as libc::mode_t) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        progress.item();
        return Ok(());
    }
    copy_file(from, to, &meta, progress)?;
    progress.item();
    Ok(())
}

const CHUNK: usize = 4 << 20;

fn copy_file(from: &Path, to: &Path, meta: &std::fs::Metadata, progress: &Progress) -> io::Result<()> {
    use std::io::{Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    let mut source = std::fs::File::open(from)?;
    let mut target = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(to)?;
    if !copy_range(&source, &target, meta.len(), progress)? {
        let mut buffer = vec![0u8; CHUNK];
        loop {
            progress.check()?;
            let read = match source.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            target.write_all(&buffer[..read])?;
            progress.wrote(read as u64);
        }
    }
    target.set_permissions(meta.permissions())?;
    if let Ok(when) = meta.modified() {
        let _ = target.set_modified(when);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_range(source: &std::fs::File, target: &std::fs::File, length: u64, progress: &Progress) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut moved: u64 = 0;
    loop {
        progress.check()?;
        let wrote = unsafe { libc::copy_file_range(source.as_raw_fd(), std::ptr::null_mut(), target.as_raw_fd(), std::ptr::null_mut(), CHUNK, 0) };
        let moved_any = moved > 0;
        match wrote {
            0 => return Ok(moved_any && moved >= length),
            1.. => {
                moved += wrote as u64;
                progress.wrote(wrote as u64);
            }
            _ => {
                let error = io::Error::last_os_error();
                match error.raw_os_error() {
                    Some(libc::EINTR) => {}
                    Some(libc::EXDEV | libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP | libc::EPERM) if !moved_any => return Ok(false),
                    _ => return Err(error),
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn copy_range(_: &std::fs::File, _: &std::fs::File, _: u64, _: &Progress) -> io::Result<bool> {
    Ok(false)
}

fn measure(path: &Path, progress: &Progress) -> io::Result<()> {
    progress.check()?;
    let meta = std::fs::symlink_metadata(path)?;
    progress.count.fetch_add(1, Ordering::Relaxed);
    if meta.is_dir() {
        for item in std::fs::read_dir(path)? {
            measure(&item?.path(), progress)?;
        }
    } else if meta.is_file() {
        progress.total.fetch_add(meta.len(), Ordering::Relaxed);
    }
    Ok(())
}

fn same_disk(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::symlink_metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev(),
        _ => true,
    }
}

pub fn move_into(from: &Path, into: &Path) -> io::Result<PathBuf> {
    move_into_with(from, into, &Progress::default())
}

pub fn move_into_with(from: &Path, into: &Path, progress: &Progress) -> io::Result<PathBuf> {
    if from.parent() == Some(into) {
        return Ok(from.to_path_buf());
    }
    if into.starts_with(from) {
        return Err(io::Error::other(crate::lang::t("A folder cannot go inside itself")));
    }
    let name = from.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    let target = unique(into, &name);
    move_to(from, &target, progress)?;
    Ok(target)
}

fn move_to(from: &Path, to: &Path, progress: &Progress) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            if let Err(error) = copy_tree(from, to, progress) {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    let _ = remove(to);
                }
                return Err(error);
            }
            remove(from)
        }
        Err(error) => Err(error),
    }
}

fn remove(path: &Path) -> io::Result<()> {
    remove_with(path, &Progress::default())
}

fn remove_with(path: &Path, progress: &Progress) -> io::Result<()> {
    progress.check()?;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return std::fs::remove_file(path);
    }
    for item in std::fs::read_dir(path)? {
        remove_with(&item?.path(), progress)?;
    }
    std::fs::remove_dir(path)
}

pub(crate) fn encode(path: &Path) -> String {
    let mut out = String::new();
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub(crate) fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(byte) = text.get(at + 1..at + 3).and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn trash_in(root: &Path, path: &Path) -> io::Result<Trashed> {
    trash_in_with(root, path, &Progress::default())
}

fn trash_in_with(root: &Path, path: &Path, progress: &Progress) -> io::Result<Trashed> {
    let (files, infos) = (root.join("files"), root.join("info"));
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&infos)?;
    let name = path.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    let (stem, ext) = split_name(&name);
    let mut n = 1;
    let (file, info) = loop {
        let pick = match n {
            1 => name.clone(),
            _ => format!("{stem}.{n}{ext}"),
        };
        let info = infos.join(format!("{pick}.trashinfo"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&info) {
            Ok(mut handle) => {
                use std::io::Write;
                let when = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
                let absolute = real(path).map_or_else(|| std::path::absolute(path), Ok)?;
                let shown = match top_real(root).and_then(|top| absolute.strip_prefix(top).ok().map(Path::to_path_buf)) {
                    Some(inside) => inside,
                    None => absolute,
                };
                write!(handle, "[Trash Info]\nPath={}\nDeletionDate={when}\n", encode(&shown))?;
                break (files.join(&pick), info);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(error) => return Err(error),
        }
    };
    if let Err(error) = move_to(path, &file, progress) {
        let _ = std::fs::remove_file(&info);
        return Err(error);
    }
    Ok(Trashed { from: path.to_path_buf(), file, info })
}

pub fn trash(path: &Path) -> io::Result<Trashed> {
    trash_in(&trash_root(), path)
}

pub fn put_back(trashed: &Trashed) -> io::Result<PathBuf> {
    put_back_with(trashed, &Progress::default())
}

fn put_back_with(trashed: &Trashed, progress: &Progress) -> io::Result<PathBuf> {
    let parent = trashed.from.parent().ok_or_else(|| io::Error::other("no folder"))?;
    std::fs::create_dir_all(parent)?;
    let name = trashed.from.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    let target = unique(parent, &name);
    move_to(&trashed.file, &target, progress)?;
    let _ = std::fs::remove_file(&trashed.info);
    Ok(target)
}

pub fn deleted_at(info: &Path) -> Option<std::time::SystemTime> {
    let text = std::fs::read_to_string(info).ok()?;
    let when = text.lines().find_map(|line| line.strip_prefix("DeletionDate="))?.trim();
    let naive = chrono::NaiveDateTime::parse_from_str(when, "%Y-%m-%dT%H:%M:%S").ok()?;
    let local = naive.and_local_timezone(chrono::Local).earliest()?;
    Some(local.into())
}

pub fn trashed_from(root: &Path, file: &Path) -> Option<Trashed> {
    if file.parent() != Some(root.join("files").as_path()) {
        return None;
    }
    let name = file.file_name()?.to_string_lossy().into_owned();
    let info = root.join("info").join(format!("{name}.trashinfo"));
    let text = std::fs::read_to_string(&info).ok()?;
    let from = PathBuf::from(decode(text.lines().find_map(|line| line.strip_prefix("Path="))?.trim()));
    let from = match top_real(root) {
        Some(top) => {
            let whole = match from.is_relative() {
                true => top.join(&from),
                false => from,
            };
            let plain = whole.components().all(|part| !matches!(part, std::path::Component::ParentDir));
            if !plain || !whole.starts_with(&top) {
                return None;
            }
            whole
        }
        None => from,
    };
    Some(Trashed { from, file: file.to_path_buf(), info })
}

pub fn empty_trash_in(root: &Path) -> io::Result<()> {
    empty_trash_with(&[root.to_path_buf()], &Progress::default())
}

fn empty_trash_with(roots: &[PathBuf], progress: &Progress) -> io::Result<()> {
    empty_trashes(roots, progress).into_iter().next().map_or(Ok(()), Err)
}

fn same_place(roots: &[PathBuf]) -> Vec<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let mut seen = std::collections::HashSet::new();
    roots.iter().filter(|root| std::fs::metadata(root).map_or(true, |meta| seen.insert((meta.dev(), meta.ino())))).cloned().collect()
}

fn empty_trashes(roots: &[PathBuf], progress: &Progress) -> Vec<io::Error> {
    let roots = same_place(roots);
    let items: Vec<PathBuf> = roots.iter().flat_map(|root| [root.join("files"), root.join("info")]).filter_map(|sub| std::fs::read_dir(sub).ok()).flatten().flatten().map(|item| item.path()).collect();
    progress.count.store(items.len() as u64, Ordering::Relaxed);
    let mut failed = Vec::new();
    for item in items {
        match remove_with(&item, progress) {
            Err(error) if progress.stopped() => {
                failed.push(error);
                break;
            }
            Err(error) if error.kind() != io::ErrorKind::NotFound => failed.push(io::Error::other(format!("{}: {error}", crate::explorer::entry::name_of(&item)))),
            _ => {}
        }
        progress.item();
    }
    failed
}

pub fn rename_all(pairs: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let mut parked: Vec<(PathBuf, &PathBuf, &PathBuf)> = Vec::new();
    for (at, (from, to)) in pairs.iter().enumerate() {
        let folder = from.parent().unwrap_or(Path::new("/"));
        let spare = folder.join(format!(".drifter-rename-{}-{at}", std::process::id()));
        if let Err(error) = std::fs::rename(from, &spare) {
            for (spare, from, _) in parked.iter().rev() {
                let _ = std::fs::rename(spare, from);
            }
            return Err(format!("{}: {error}", crate::explorer::entry::name_of(from)));
        }
        parked.push((spare, from, to));
    }
    let mut failed = Vec::new();
    for (spare, from, to) in &parked {
        let target = match std::fs::symlink_metadata(to) {
            Ok(_) => from,
            Err(_) => to,
        };
        if let Err(error) = std::fs::rename(spare, target) {
            failed.push(format!("{}: {error}", crate::explorer::entry::name_of(from)));
        } else if target == from {
            failed.push(crate::lang::tf("{} already exists here", &[&crate::explorer::entry::name_of(to)]));
        }
    }
    match failed.is_empty() {
        true => Ok(()),
        false => Err(failed.join(", ")),
    }
}

pub fn rename(path: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(crate::lang::t("A name cannot be empty").to_string());
    }
    if name.contains('/') || name.contains('\0') {
        return Err(crate::lang::t("A name cannot hold a slash").to_string());
    }
    let parent = path.parent().ok_or_else(|| crate::lang::t("This item has no folder").to_string())?;
    let target = parent.join(name);
    if target == path {
        return Ok(target);
    }
    let same_but_case = name.to_lowercase() == crate::explorer::entry::name_of(path).to_lowercase();
    if !same_but_case && std::fs::symlink_metadata(&target).is_ok() {
        return Err(crate::lang::tf("{} already exists here", &[&name]));
    }
    std::fs::rename(path, &target).map_err(|error| error.to_string())?;
    Ok(target)
}

pub fn new_folder(dir: &Path) -> io::Result<PathBuf> {
    let path = unique(dir, crate::lang::t("untitled folder"));
    std::fs::create_dir(&path)?;
    Ok(path)
}

pub fn new_file(dir: &Path) -> io::Result<PathBuf> {
    let path = unique(dir, &format!("{}.txt", crate::lang::t("untitled")));
    std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    Ok(path)
}

pub fn run(job: Job, progress: &Progress) -> Done {
    run_with(&trash_root(), job, progress)
}

fn count_moves(paths: &[PathBuf], into: impl Fn(&Path) -> PathBuf, progress: &Progress) -> io::Result<()> {
    progress.counting.store(true, Ordering::Relaxed);
    let far: Vec<&PathBuf> = paths.iter().filter(|path| !same_disk(path, &into(path))).collect();
    match far.is_empty() {
        true => progress.count.store(paths.len() as u64, Ordering::Relaxed),
        false => {
            for path in far {
                measure(path, progress)?;
            }
        }
    }
    progress.counting.store(false, Ordering::Relaxed);
    Ok(())
}

fn each_moved(progress: &Progress, far: bool) {
    if !far {
        progress.item();
    }
}

pub fn run_with(root: &Path, job: Job, progress: &Progress) -> Done {
    let words = job.doing();
    let mut done = Done { id: 0, stopped: false, undo: None, made: Vec::new(), failed: Vec::new(), words, go: None };
    let fail = |path: &Path, error: io::Error| format!("{}: {error}", crate::explorer::entry::name_of(path));
    let counted = match &job {
        Job::Copy { from, .. } => {
            progress.counting.store(true, Ordering::Relaxed);
            let all = from.iter().try_for_each(|path| measure(path, progress));
            progress.counting.store(false, Ordering::Relaxed);
            all
        }
        Job::Move { from, into, .. } => count_moves(from, |_| into.clone(), progress),
        Job::Trash(paths) => count_moves(paths, |path| trash_for(root, path), progress),
        Job::Delete(paths) | Job::Restore(paths) => {
            progress.count.store(paths.len() as u64, Ordering::Relaxed);
            Ok(())
        }
        Job::Compress(_) | Job::Mount { .. } | Job::Unmount { .. } | Job::Eject { .. } => Ok(()),
        Job::Extract(paths) => {
            progress.count.store(paths.len() as u64, Ordering::Relaxed);
            Ok(())
        }
        Job::EmptyTrash | Job::Undo(_) => Ok(()),
    };
    if counted.is_err() && progress.stopped() {
        done.stopped = true;
        return done;
    }
    let far = progress.total.load(Ordering::Relaxed) > 0;
    match job {
        Job::Copy { from, into, clash } => {
            let mut replaced = Vec::new();
            for path in &from {
                let spot = Spot { root, into: &into, clash, made: &done.made, measured: true };
                let result = match place(&spot, path, progress) {
                    Ok(Placed::Free) => copy_into_with(path, &into, progress).map(Some),
                    Ok(Placed::Skip) => Ok(None),
                    Ok(Placed::Over(target, old)) => match copy_onto(path, &target, progress) {
                        Ok(()) => {
                            replaced.push(old);
                            Ok(Some(target))
                        }
                        Err(error) => {
                            let _ = put_back(&old);
                            Err(error)
                        }
                    },
                    Err(error) => Err(error),
                };
                match result {
                    Ok(Some(made)) => done.made.push(made),
                    Ok(None) => {}
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
            done.undo = with_old((!done.made.is_empty()).then(|| Undo::Made(done.made.clone())), replaced);
        }
        Job::Move { from, into, clash } => {
            let mut moved = Vec::new();
            let mut replaced = Vec::new();
            for path in &from {
                let spot = Spot { root, into: &into, clash, made: &done.made, measured: !same_disk(path, &into) };
                let result = match place(&spot, path, progress) {
                    Ok(Placed::Free) => move_into_with(path, &into, progress).map(Some),
                    Ok(Placed::Skip) => Ok(None),
                    Ok(Placed::Over(target, old)) => match move_to(path, &target, progress) {
                        Ok(()) => {
                            replaced.push(old);
                            Ok(Some(target))
                        }
                        Err(error) => {
                            let _ = put_back(&old);
                            Err(error)
                        }
                    },
                    Err(error) => Err(error),
                };
                match result {
                    Ok(Some(made)) if &made == path => {}
                    Ok(Some(made)) => {
                        moved.push((path.clone(), made.clone()));
                        done.made.push(made);
                    }
                    Ok(None) => {}
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                each_moved(progress, far);
            }
            done.undo = with_old((!moved.is_empty()).then_some(Undo::Moved(moved)), replaced);
        }
        Job::Trash(paths) => {
            let mut gone = Vec::new();
            for path in &paths {
                match trash_in_with(&trash_for(root, path), path, progress) {
                    Ok(trashed) => gone.push(trashed),
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                each_moved(progress, far);
            }
            done.undo = (!gone.is_empty()).then_some(Undo::Trashed(gone));
        }
        Job::Delete(paths) => {
            for path in &paths {
                match remove_with(path, progress) {
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                    Ok(()) => {}
                }
                if let Some(trashed) = trashed_from(&root_of(path).unwrap_or_else(|| root.to_path_buf()), path) {
                    let _ = std::fs::remove_file(trashed.info);
                }
                progress.item();
            }
        }
        Job::Restore(paths) => {
            for path in &paths {
                match trashed_from(&root_of(path).unwrap_or_else(|| root.to_path_buf()), path).ok_or_else(|| io::Error::other(crate::lang::t("Its first place is not known"))).and_then(|t| put_back_with(&t, progress)) {
                    Ok(made) => done.made.push(made),
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                progress.item();
            }
        }
        Job::EmptyTrash => {
            let roots: Vec<PathBuf> = std::iter::once(root.to_path_buf()).chain(other_trashes()).collect();
            if !progress.stopped() {
                done.failed.extend(empty_trashes(&roots, progress).into_iter().filter(|_| !progress.stopped()).map(|error| error.to_string()));
            }
        }
        Job::Undo(undo) => undo_with(root, undo, &Progress::default(), &mut done),
        Job::Compress(paths) => {
            match super::pack::compress(&paths, progress) {
                Ok(made) => done.made.push(made),
                Err(_) if progress.stopped() => {}
                Err(error) => done.failed.push(fail(paths.first().map_or(root, PathBuf::as_path), error)),
            }
            done.undo = (!done.made.is_empty()).then(|| Undo::Made(done.made.clone()));
        }
        Job::Mount { device, name } => match super::drives::mount(&device) {
            Ok(point) => done.go = Some(point),
            Err(error) => done.failed.push(format!("{name}: {error}")),
        },
        Job::Unmount { device, name } => {
            if let Err(error) = super::drives::unmount(&device) {
                done.failed.push(format!("{name}: {error}"));
            }
        }
        Job::Eject { device, disk, name, mounted } => {
            if let Err(error) = super::drives::eject(&device, disk.as_deref(), mounted) {
                done.failed.push(format!("{name}: {error}"));
            }
        }
        Job::Extract(paths) => {
            for path in &paths {
                let into = path.parent().unwrap_or(root);
                match super::pack::extract(path, into, progress) {
                    Ok(made) => done.made.push(made),
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                progress.item();
            }
            done.undo = (!done.made.is_empty()).then(|| Undo::Made(done.made.clone()));
        }
    }
    done.stopped = progress.stopped();
    done
}

fn undo_with(root: &Path, undo: Undo, progress: &Progress, done: &mut Done) {
    let fail = |path: &Path, error: io::Error| format!("{}: {error}", crate::explorer::entry::name_of(path));
    match undo {
        Undo::Made(paths) => {
            for path in &paths {
                if let Err(error) = trash_in_with(&trash_for(root, path), path, progress) {
                    done.failed.push(fail(path, error));
                }
            }
        }
        Undo::Moved(pairs) => {
            for (was, now) in &pairs {
                let back = was.parent().map(|parent| unique(parent, &crate::explorer::entry::name_of(was)));
                match back {
                    Some(back) => {
                        if let Err(error) = move_to(now, &back, progress) {
                            done.failed.push(fail(now, error));
                        }
                    }
                    None => done.failed.push(fail(now, io::Error::other("no folder"))),
                }
            }
        }
        Undo::Renamed(was, now) => {
            if let Err(error) = std::fs::rename(&now, &was) {
                done.failed.push(fail(&now, error));
            }
        }
        Undo::RenamedAll(pairs) => {
            let back: Vec<(PathBuf, PathBuf)> = pairs.into_iter().map(|(was, now)| (now, was)).collect();
            if let Err(error) = rename_all(&back) {
                done.failed.push(error);
            }
        }
        Undo::Trashed(gone) => {
            for trashed in &gone {
                match put_back_with(trashed, progress) {
                    Ok(made) => done.made.push(made),
                    Err(error) => done.failed.push(fail(&trashed.file, error)),
                }
            }
        }
        Undo::Then(first, replaced) => {
            undo_with(root, *first, progress, done);
            for trashed in &replaced {
                match put_back_with(trashed, progress) {
                    Ok(made) => done.made.push(made),
                    Err(error) => done.failed.push(fail(&trashed.file, error)),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Place(PathBuf);

    impl Place {
        fn new(tag: &str) -> Place {
            let dir = std::env::temp_dir().join(format!("drifter-ops-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("work")).unwrap();
            Place(dir)
        }

        fn work(&self) -> PathBuf {
            self.0.join("work")
        }

        fn trash(&self) -> PathBuf {
            self.0.join("Trash")
        }
    }

    impl Drop for Place {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_copy_counts_first_then_reports_every_byte() {
        let place = Place::new("progress");
        let from = place.work().join("from");
        std::fs::create_dir_all(from.join("inner")).unwrap();
        std::fs::write(from.join("big.bin"), vec![7u8; 9 << 20]).unwrap();
        std::fs::write(from.join("inner/small.txt"), "hi").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options().write(true).open(from.join("big.bin")).unwrap().set_modified(old).unwrap();
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let progress = Progress::default();
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone()], into: into.clone(), clash: Clash::KeepBoth }, &progress);
        assert!(done.failed.is_empty() && !done.stopped);
        assert_eq!(progress.total.load(Ordering::Relaxed), (9 << 20) + 2);
        assert_eq!(progress.bytes.load(Ordering::Relaxed), (9 << 20) + 2);
        assert_eq!(progress.items.load(Ordering::Relaxed), progress.count.load(Ordering::Relaxed));
        assert_eq!(progress.share(), Some(1.0));
        assert_eq!(std::fs::read(into.join("from/big.bin")).unwrap().len(), 9 << 20);
        assert_eq!(std::fs::metadata(into.join("from/big.bin")).unwrap().modified().unwrap(), old);
    }

    #[test]
    fn a_stopped_copy_leaves_no_half_file() {
        let place = Place::new("stop");
        let from = place.work().join("big.bin");
        std::fs::write(&from, vec![1u8; 64 << 20]).unwrap();
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let progress = std::sync::Arc::new(Progress::default());
        progress.stop.store(true, Ordering::Relaxed);
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone()], into: into.clone(), clash: Clash::KeepBoth }, &progress);
        assert!(done.stopped && done.made.is_empty() && done.failed.is_empty());
        let progress = std::sync::Arc::new(Progress::default());
        let watching = progress.clone();
        let stopper = std::thread::spawn(move || {
            while watching.bytes.load(Ordering::Relaxed) == 0 {
                std::thread::yield_now();
            }
            watching.stop.store(true, Ordering::Relaxed);
        });
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone(), from.clone()], into: into.clone(), clash: Clash::KeepBoth }, &progress);
        stopper.join().unwrap();
        assert!(done.stopped && done.failed.is_empty());
        assert!(progress.bytes.load(Ordering::Relaxed) < 128 << 20);
        let left: Vec<PathBuf> = std::fs::read_dir(&into).unwrap().map(|item| item.unwrap().path()).collect();
        assert_eq!(left, done.made);
        assert!(from.exists());
    }

    #[test]
    fn a_delete_counts_items_and_can_stop() {
        let place = Place::new("delete");
        let paths: Vec<PathBuf> = (0..4).map(|n| place.work().join(format!("{n}.txt"))).collect();
        for path in &paths {
            std::fs::write(path, "x").unwrap();
        }
        let progress = Progress::default();
        run_with(&place.trash(), Job::Delete(paths[..2].to_vec()), &progress);
        assert_eq!(progress.share(), Some(1.0));
        let progress = Progress::default();
        progress.stop.store(true, Ordering::Relaxed);
        let done = run_with(&place.trash(), Job::Delete(paths[2..].to_vec()), &progress);
        assert!(done.stopped && paths[2].exists() && paths[3].exists() && !paths[0].exists());
    }

    #[test]
    fn a_clash_can_replace_skip_or_keep_both() {
        let place = Place::new("clash");
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let from = place.work().join("a.txt");
        std::fs::write(&from, "new").unwrap();
        std::fs::write(into.join("a.txt"), "old").unwrap();
        assert_eq!(clashes(&[from.clone()], &into), [into.join("a.txt")]);
        assert!(clashes(&[from.clone()], &place.work()).is_empty());
        let copy = |clash| Job::Copy { from: vec![from.clone()], into: into.clone(), clash };
        let skipped = run_with(&place.trash(), copy(Clash::Skip), &Progress::default());
        assert!(skipped.made.is_empty() && skipped.undo.is_none());
        assert_eq!(std::fs::read_to_string(into.join("a.txt")).unwrap(), "old");
        let both = run_with(&place.trash(), copy(Clash::KeepBoth), &Progress::default());
        assert_eq!(both.made, [into.join("a 2.txt")]);
        let replaced = run_with(&place.trash(), copy(Clash::Replace), &Progress::default());
        assert_eq!(replaced.made, [into.join("a.txt")]);
        assert_eq!(std::fs::read_to_string(into.join("a.txt")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(place.trash().join("files/a.txt")).unwrap(), "old");
        run_with(&place.trash(), Job::Undo(replaced.undo.unwrap()), &Progress::default());
        assert_eq!(std::fs::read_to_string(into.join("a.txt")).unwrap(), "old");
        assert!(from.exists());
    }

    #[test]
    fn a_move_can_replace_and_undo_brings_both_back() {
        let place = Place::new("clashmove");
        let into = place.work().join("into");
        std::fs::create_dir_all(into.join("box")).unwrap();
        std::fs::write(into.join("box/old.txt"), "old").unwrap();
        let from = place.work().join("box");
        std::fs::create_dir(&from).unwrap();
        std::fs::write(from.join("new.txt"), "new").unwrap();
        let done = run_with(&place.trash(), Job::Move { from: vec![from.clone()], into: into.clone(), clash: Clash::Replace }, &Progress::default());
        assert!(done.failed.is_empty(), "{:?}", done.failed);
        assert!(!from.exists() && into.join("box/new.txt").exists() && !into.join("box/old.txt").exists());
        run_with(&place.trash(), Job::Undo(done.undo.unwrap()), &Progress::default());
        assert!(from.join("new.txt").exists() && into.join("box/old.txt").exists());
        let nested = place.work().join("nest/nest");
        std::fs::create_dir_all(&nested).unwrap();
        let done = run_with(&place.trash(), Job::Move { from: vec![nested.clone()], into: place.work(), clash: Clash::Replace }, &Progress::default());
        assert_eq!(done.failed.len(), 1);
        assert!(nested.exists());
    }

    #[test]
    fn a_disk_trash_keeps_paths_relative_to_the_disk() {
        let place = Place::new("disktrash");
        let top = place.work().join("stick");
        let root = top.join(format!(".Trash-{}", uid()));
        std::fs::create_dir_all(top.join("photos")).unwrap();
        let file = top.join("photos/cat.png");
        std::fs::write(&file, "meow").unwrap();
        assert!(usable(&root));
        let trashed = trash_in(&root, &file).unwrap();
        let info = std::fs::read_to_string(&trashed.info).unwrap();
        assert!(info.contains("\nPath=photos/cat.png\n"), "{info}");
        assert_eq!(root_of(&trashed.file), Some(root.clone()));
        assert!(in_trash(&trashed.file));
        assert!(!in_trash(&file));
        assert_eq!(trashed_from(&root, &trashed.file).map(|t| t.from), Some(file.clone()));
        let when = deleted_at(&trashed.info).unwrap();
        assert!(when.elapsed().unwrap() < std::time::Duration::from_secs(60));
        let done = run_with(&place.trash(), Job::Restore(vec![trashed.file.clone()]), &Progress::default());
        assert_eq!(done.made, [file.clone()]);
        assert!(!trashed.info.exists());
        assert_eq!(trash_for(&place.trash(), &file), place.trash());
        let shared = top.join(".Trash");
        std::fs::create_dir(&shared).unwrap();
        assert_eq!(top_of_root(&shared.join(uid().to_string())), Some(top.clone()));
        assert_eq!(top_of_root(&place.trash()), None);
    }

    #[test]
    fn replace_never_trashes_an_item_this_job_just_placed() {
        let place = Place::new("twonames");
        let into = place.work().join("into");
        for dir in ["a", "b", "into"] {
            std::fs::create_dir_all(place.work().join(dir)).unwrap();
        }
        std::fs::write(place.work().join("a/x.txt"), "A").unwrap();
        std::fs::write(place.work().join("b/x.txt"), "B").unwrap();
        std::fs::write(into.join("x.txt"), "OLD").unwrap();
        let from = vec![place.work().join("a/x.txt"), place.work().join("b/x.txt")];
        let done = run_with(&place.trash(), Job::Move { from, into: into.clone(), clash: Clash::Replace }, &Progress::default());
        assert!(done.failed.is_empty(), "{:?}", done.failed);
        assert_eq!(std::fs::read_to_string(into.join("x.txt")).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(into.join("x 2.txt")).unwrap(), "B");
        assert_eq!(std::fs::read_dir(place.trash().join("files")).unwrap().count(), 1);
        let undone = run_with(&place.trash(), Job::Undo(done.undo.unwrap()), &Progress::default());
        assert!(undone.failed.is_empty(), "{:?}", undone.failed);
        assert_eq!(std::fs::read_to_string(place.work().join("a/x.txt")).unwrap(), "A");
        assert_eq!(std::fs::read_to_string(place.work().join("b/x.txt")).unwrap(), "B");
        assert_eq!(std::fs::read_to_string(into.join("x.txt")).unwrap(), "OLD");
    }

    #[test]
    fn skip_keeps_the_bar_whole() {
        let place = Place::new("skipbar");
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        std::fs::write(place.work().join("a"), "").unwrap();
        std::fs::write(place.work().join("b"), "").unwrap();
        std::fs::write(into.join("a"), "").unwrap();
        let progress = Progress::default();
        run_with(&place.trash(), Job::Copy { from: vec![place.work().join("a"), place.work().join("b")], into, clash: Clash::Skip }, &progress);
        assert_eq!(progress.share(), Some(1.0));
    }

    #[test]
    fn empty_trash_keeps_going_past_a_failure() {
        use std::os::unix::fs::PermissionsExt;
        let place = Place::new("emptyall");
        let locked = place.trash().join("files/locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("in.txt"), "x").unwrap();
        std::fs::write(place.trash().join("files/free.txt"), "x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        let failed = empty_trashes(&[place.trash(), place.trash()], &Progress::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(failed.len(), 1);
        assert!(!place.trash().join("files/free.txt").exists());
    }

    #[test]
    fn a_pipe_inside_a_folder_is_made_again_not_read() {
        use std::os::unix::fs::FileTypeExt;
        let place = Place::new("fifo");
        let from = place.work().join("box");
        std::fs::create_dir(&from).unwrap();
        let name = std::ffi::CString::new(from.join("pipe").to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let done = run_with(&place.trash(), Job::Copy { from: vec![from], into: into.clone(), clash: Clash::KeepBoth }, &Progress::default());
        assert!(done.failed.is_empty(), "{:?}", done.failed);
        assert!(std::fs::symlink_metadata(into.join("box/pipe")).unwrap().file_type().is_fifo());
    }

    #[test]
    fn only_top_level_trash_items_have_trash_info() {
        let place = Place::new("nested");
        let proj = place.work().join("proj");
        std::fs::create_dir(&proj).unwrap();
        std::fs::write(proj.join("a.txt"), "inner").unwrap();
        std::fs::write(place.work().join("a.txt"), "outer").unwrap();
        trash_in(&place.trash(), &proj).unwrap();
        trash_in(&place.trash(), &place.work().join("a.txt")).unwrap();
        let nested = place.trash().join("files/proj/a.txt");
        assert!(trashed_from(&place.trash(), &nested).is_none());
        let done = run_with(&place.trash(), Job::Restore(vec![nested.clone()]), &Progress::default());
        assert_eq!(done.failed.len(), 1);
        assert!(nested.exists() && place.trash().join("info/a.txt.trashinfo").exists());
        let elsewhere = place.work().join("b");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("a.txt"), "x").unwrap();
        run_with(&place.trash(), Job::Delete(vec![elsewhere.join("a.txt")]), &Progress::default());
        assert!(place.trash().join("info/a.txt.trashinfo").exists());
    }

    #[test]
    fn a_disk_trash_cannot_send_things_outside_the_disk() {
        let place = Place::new("escape");
        let top = place.work().join("stick");
        let root = top.join(format!(".Trash-{}", uid()));
        std::fs::create_dir_all(root.join("files")).unwrap();
        std::fs::create_dir_all(root.join("info")).unwrap();
        std::fs::write(root.join("files/x"), "x").unwrap();
        std::fs::write(root.join("info/x.trashinfo"), "[Trash Info]\nPath=../../outside/x\n").unwrap();
        assert!(trashed_from(&root, &root.join("files/x")).is_none());
        std::fs::write(root.join("info/x.trashinfo"), "[Trash Info]\nPath=/etc/x\n").unwrap();
        assert!(trashed_from(&root, &root.join("files/x")).is_none());
        assert!(trusted(&root));
        let fake = place.work().join("fake");
        std::fs::create_dir_all(fake.join("files")).unwrap();
        let link = top.join(".Trash-link");
        std::os::unix::fs::symlink(&fake, &link).unwrap();
        assert!(!trusted(&link));
    }

    #[test]
    fn a_copy_in_the_same_folder_gets_a_new_name() {
        let place = Place::new("copy");
        let file = place.work().join("note.txt");
        std::fs::write(&file, "hi").unwrap();
        let first = copy_into(&file, &place.work()).unwrap();
        let second = copy_into(&file, &place.work()).unwrap();
        assert_eq!(first, place.work().join("note copy.txt"));
        assert_eq!(second, place.work().join("note copy 2.txt"));
        assert_eq!(std::fs::read_to_string(second).unwrap(), "hi");
    }

    #[test]
    fn folders_copy_with_everything_inside() {
        let place = Place::new("tree");
        let from = place.work().join("a");
        std::fs::create_dir_all(from.join("b")).unwrap();
        std::fs::write(from.join("b/c.txt"), "deep").unwrap();
        let into = place.work().join("z");
        std::fs::create_dir(&into).unwrap();
        let made = copy_into(&from, &into).unwrap();
        assert_eq!(std::fs::read_to_string(made.join("b/c.txt")).unwrap(), "deep");
        assert!(copy_into(&from, &from.join("b")).is_err());
    }

    #[test]
    fn trash_and_undo_bring_the_file_back() {
        let place = Place::new("trash");
        let file = place.work().join("old file.txt");
        std::fs::write(&file, "keep").unwrap();
        let done = run_with(&place.trash(), Job::Trash(vec![file.clone()]), &Progress::default());
        assert!(done.failed.is_empty(), "{:?}", done.failed);
        assert!(!file.exists());
        let info = std::fs::read_to_string(place.trash().join("info/old file.txt.trashinfo")).unwrap();
        assert!(info.contains("old%20file.txt"), "{info}");
        let found = trashed_from(&place.trash(), &place.trash().join("files/old file.txt")).unwrap();
        assert_eq!(found.from, std::path::absolute(&file).unwrap());
        let undone = run_with(&place.trash(), Job::Undo(done.undo.unwrap()), &Progress::default());
        assert!(undone.failed.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
        assert!(!place.trash().join("info/old file.txt.trashinfo").exists());
    }

    #[test]
    fn two_trashed_files_with_one_name_both_stay() {
        let place = Place::new("twice");
        for _ in 0..2 {
            std::fs::write(place.work().join("a.txt"), "x").unwrap();
            run_with(&place.trash(), Job::Trash(vec![place.work().join("a.txt")]), &Progress::default());
        }
        assert!(place.trash().join("files/a.txt").exists());
        assert!(place.trash().join("files/a.2.txt").exists());
        run_with(&place.trash(), Job::EmptyTrash, &Progress::default());
        assert_eq!(std::fs::read_dir(place.trash().join("files")).unwrap().count(), 0);
    }

    #[test]
    fn a_move_can_be_undone() {
        let place = Place::new("move");
        let file = place.work().join("m.txt");
        std::fs::write(&file, "x").unwrap();
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let done = run_with(&place.trash(), Job::Move { from: vec![file.clone()], into: into.clone(), clash: Clash::KeepBoth }, &Progress::default());
        assert!(into.join("m.txt").exists() && !file.exists());
        run_with(&place.trash(), Job::Undo(done.undo.unwrap()), &Progress::default());
        assert!(file.exists() && !into.join("m.txt").exists());
    }

    #[test]
    fn renames_refuse_bad_names() {
        let place = Place::new("rename");
        let file = place.work().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        std::fs::write(place.work().join("b.txt"), "x").unwrap();
        assert!(rename(&file, "").is_err());
        assert!(rename(&file, "x/y").is_err());
        assert!(rename(&file, "b.txt").is_err());
        assert_eq!(rename(&file, " c.txt ").unwrap(), place.work().join("c.txt"));
    }

    #[test]
    fn new_folders_never_collide() {
        let place = Place::new("new");
        let one = new_folder(&place.work()).unwrap();
        let two = new_folder(&place.work()).unwrap();
        assert_ne!(one, two);
        assert!(two.ends_with("untitled folder 2"));
    }

    #[test]
    fn paths_round_trip_through_the_trash_info() {
        let path = Path::new("/home/me/a b%c/ü.txt");
        assert_eq!(decode(&encode(path)), path.to_string_lossy());
    }
}
