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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Copy { from: Vec<PathBuf>, into: PathBuf },
    Move { from: Vec<PathBuf>, into: PathBuf },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
    Restore(Vec<PathBuf>),
    EmptyTrash,
    Undo(Undo),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    Made(Vec<PathBuf>),
    Moved(Vec<(PathBuf, PathBuf)>),
    Renamed(PathBuf, PathBuf),
    Trashed(Vec<Trashed>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub id: u64,
    pub stopped: bool,
    pub undo: Option<Undo>,
    pub made: Vec<PathBuf>,
    pub failed: Vec<String>,
    pub words: String,
}

impl Job {
    pub fn count(&self) -> usize {
        match self {
            Job::Copy { from, .. } | Job::Move { from, .. } => from.len(),
            Job::Trash(paths) | Job::Delete(paths) | Job::Restore(paths) => paths.len(),
            Job::EmptyTrash => 0,
            Job::Undo(_) => 1,
        }
    }

    pub fn doing(&self) -> String {
        use crate::explorer::entry::{items, name_of};
        let what = match self {
            Job::Copy { from, .. } | Job::Move { from, .. } | Job::Trash(from) | Job::Delete(from) | Job::Restore(from) => match from.as_slice() {
                [one] => name_of(one),
                many => items(many.len()),
            },
            Job::EmptyTrash | Job::Undo(_) => String::new(),
        };
        match self {
            Job::Copy { into, .. } => crate::lang::tf("Copying {} to {}", &[&what, &name_of(into)]),
            Job::Move { into, .. } => crate::lang::tf("Moving {} to {}", &[&what, &name_of(into)]),
            Job::Trash(_) => crate::lang::tf("Moving {} to the Trash", &[&what]),
            Job::Delete(_) => crate::lang::tf("Deleting {}", &[&what]),
            Job::Restore(_) => crate::lang::tf("Putting back {}", &[&what]),
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
    path.starts_with(trash_files())
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
    if let Err(error) = copy_tree(from, &target, progress) {
        if error.kind() != io::ErrorKind::AlreadyExists {
            let _ = remove(&target);
        }
        return Err(error);
    }
    Ok(target)
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
    if !copy_range(&source, &target, progress)? {
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
fn copy_range(source: &std::fs::File, target: &std::fs::File, progress: &Progress) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut moved_any = false;
    loop {
        progress.check()?;
        let wrote = unsafe { libc::copy_file_range(source.as_raw_fd(), std::ptr::null_mut(), target.as_raw_fd(), std::ptr::null_mut(), CHUNK, 0) };
        match wrote {
            0 => return Ok(true),
            1.. => {
                moved_any = true;
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
fn copy_range(_: &std::fs::File, _: &std::fs::File, _: &Progress) -> io::Result<bool> {
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
                let absolute = std::path::absolute(path)?;
                write!(handle, "[Trash Info]\nPath={}\nDeletionDate={when}\n", encode(&absolute))?;
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

pub fn trashed_from(root: &Path, file: &Path) -> Option<Trashed> {
    let name = file.file_name()?.to_string_lossy().into_owned();
    let info = root.join("info").join(format!("{name}.trashinfo"));
    let text = std::fs::read_to_string(&info).ok()?;
    let from = text.lines().find_map(|line| line.strip_prefix("Path="))?;
    Some(Trashed { from: PathBuf::from(decode(from.trim())), file: file.to_path_buf(), info })
}

pub fn empty_trash_in(root: &Path) -> io::Result<()> {
    empty_trash_with(root, &Progress::default())
}

fn empty_trash_with(root: &Path, progress: &Progress) -> io::Result<()> {
    let items: Vec<PathBuf> = ["files", "info"].iter().filter_map(|sub| std::fs::read_dir(root.join(sub)).ok()).flatten().flatten().map(|item| item.path()).collect();
    progress.count.store(items.len() as u64, Ordering::Relaxed);
    for item in items {
        remove_with(&item, progress)?;
        progress.item();
    }
    Ok(())
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

fn count_moves(paths: &[PathBuf], into: &Path, progress: &Progress) -> io::Result<()> {
    progress.counting.store(true, Ordering::Relaxed);
    let far: Vec<&PathBuf> = paths.iter().filter(|path| !same_disk(path, into)).collect();
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
    let mut done = Done { id: 0, stopped: false, undo: None, made: Vec::new(), failed: Vec::new(), words };
    let fail = |path: &Path, error: io::Error| format!("{}: {error}", crate::explorer::entry::name_of(path));
    let counted = match &job {
        Job::Copy { from, .. } => {
            progress.counting.store(true, Ordering::Relaxed);
            let all = from.iter().try_for_each(|path| measure(path, progress));
            progress.counting.store(false, Ordering::Relaxed);
            all
        }
        Job::Move { from, into } => count_moves(from, into, progress),
        Job::Trash(paths) => count_moves(paths, root, progress),
        Job::Delete(paths) | Job::Restore(paths) => {
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
        Job::Copy { from, into } => {
            for path in &from {
                match copy_into_with(path, &into, progress) {
                    Ok(made) => done.made.push(made),
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
            done.undo = (!done.made.is_empty()).then(|| Undo::Made(done.made.clone()));
        }
        Job::Move { from, into } => {
            let mut moved = Vec::new();
            for path in &from {
                match move_into_with(path, &into, progress) {
                    Ok(made) if &made == path => {}
                    Ok(made) => {
                        moved.push((path.clone(), made.clone()));
                        done.made.push(made);
                    }
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                each_moved(progress, far);
            }
            done.undo = (!moved.is_empty()).then_some(Undo::Moved(moved));
        }
        Job::Trash(paths) => {
            let mut gone = Vec::new();
            for path in &paths {
                match trash_in_with(root, path, progress) {
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
                if let Some(trashed) = trashed_from(root, path) {
                    let _ = std::fs::remove_file(trashed.info);
                }
                progress.item();
            }
        }
        Job::Restore(paths) => {
            for path in &paths {
                match trashed_from(root, path).ok_or_else(|| io::Error::other(crate::lang::t("Its first place is not known"))).and_then(|t| put_back_with(&t, progress)) {
                    Ok(made) => done.made.push(made),
                    Err(_) if progress.stopped() => break,
                    Err(error) => done.failed.push(fail(path, error)),
                }
                progress.item();
            }
        }
        Job::EmptyTrash => {
            if let Err(error) = empty_trash_with(root, progress)
                && !progress.stopped()
            {
                done.failed.push(fail(root, error));
            }
        }
        Job::Undo(undo) => match undo {
            Undo::Made(paths) => {
                for path in &paths {
                    if let Err(error) = trash_in_with(root, path, progress) {
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
            Undo::Trashed(gone) => {
                for trashed in &gone {
                    match put_back_with(trashed, progress) {
                        Ok(made) => done.made.push(made),
                        Err(error) => done.failed.push(fail(&trashed.file, error)),
                    }
                }
            }
        },
    }
    done.stopped = progress.stopped();
    done
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
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone()], into: into.clone() }, &progress);
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
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone()], into: into.clone() }, &progress);
        assert!(done.stopped && done.made.is_empty() && done.failed.is_empty());
        let progress = std::sync::Arc::new(Progress::default());
        let watching = progress.clone();
        let stopper = std::thread::spawn(move || {
            while watching.bytes.load(Ordering::Relaxed) == 0 {
                std::thread::yield_now();
            }
            watching.stop.store(true, Ordering::Relaxed);
        });
        let done = run_with(&place.trash(), Job::Copy { from: vec![from.clone(), from.clone()], into: into.clone() }, &progress);
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
        let done = run_with(&place.trash(), Job::Move { from: vec![file.clone()], into: into.clone() }, &Progress::default());
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
