use std::io;
use std::path::{Path, PathBuf};

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
        let count = self.count();
        match self {
            Job::Copy { .. } => crate::lang::tf("Copying {}", &[&crate::explorer::entry::items(count)]),
            Job::Move { .. } => crate::lang::tf("Moving {}", &[&crate::explorer::entry::items(count)]),
            Job::Trash(_) => crate::lang::tf("Moving {} to the Trash", &[&crate::explorer::entry::items(count)]),
            Job::Delete(_) => crate::lang::tf("Deleting {}", &[&crate::explorer::entry::items(count)]),
            Job::Restore(_) => crate::lang::tf("Putting back {}", &[&crate::explorer::entry::items(count)]),
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
    copy_tree(from, &target)?;
    Ok(target)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        return std::os::unix::fs::symlink(std::fs::read_link(from)?, to);
    }
    if meta.is_dir() {
        std::fs::create_dir(to)?;
        for item in std::fs::read_dir(from)? {
            let item = item?;
            copy_tree(&item.path(), &to.join(item.file_name()))?;
        }
        return std::fs::set_permissions(to, meta.permissions());
    }
    std::fs::copy(from, to).map(|_| ())
}

pub fn move_into(from: &Path, into: &Path) -> io::Result<PathBuf> {
    if from.parent() == Some(into) {
        return Ok(from.to_path_buf());
    }
    if into.starts_with(from) {
        return Err(io::Error::other(crate::lang::t("A folder cannot go inside itself")));
    }
    let name = from.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    let target = unique(into, &name);
    move_to(from, &target)?;
    Ok(target)
}

fn move_to(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {
            copy_tree(from, to)?;
            remove(from)
        }
        Err(error) => Err(error),
    }
}

fn remove(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path)?.is_dir() {
        true => std::fs::remove_dir_all(path),
        false => std::fs::remove_file(path),
    }
}

fn encode(path: &Path) -> String {
    let mut out = String::new();
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decode(text: &str) -> String {
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
    if let Err(error) = move_to(path, &file) {
        let _ = std::fs::remove_file(&info);
        return Err(error);
    }
    Ok(Trashed { from: path.to_path_buf(), file, info })
}

pub fn trash(path: &Path) -> io::Result<Trashed> {
    trash_in(&trash_root(), path)
}

pub fn put_back(trashed: &Trashed) -> io::Result<PathBuf> {
    let parent = trashed.from.parent().ok_or_else(|| io::Error::other("no folder"))?;
    std::fs::create_dir_all(parent)?;
    let name = trashed.from.file_name().ok_or_else(|| io::Error::other("no name"))?.to_string_lossy().into_owned();
    let target = unique(parent, &name);
    move_to(&trashed.file, &target)?;
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
    for sub in ["files", "info"] {
        let Ok(items) = std::fs::read_dir(root.join(sub)) else {
            continue;
        };
        for item in items.flatten() {
            remove(&item.path())?;
        }
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

pub fn run(job: Job) -> Done {
    run_with(&trash_root(), job)
}

pub fn run_with(root: &Path, job: Job) -> Done {
    let words = job.doing();
    let mut done = Done { undo: None, made: Vec::new(), failed: Vec::new(), words };
    let fail = |path: &Path, error: io::Error| format!("{}: {error}", crate::explorer::entry::name_of(path));
    match job {
        Job::Copy { from, into } => {
            for path in &from {
                match copy_into(path, &into) {
                    Ok(made) => done.made.push(made),
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
            done.undo = (!done.made.is_empty()).then(|| Undo::Made(done.made.clone()));
        }
        Job::Move { from, into } => {
            let mut moved = Vec::new();
            for path in &from {
                match move_into(path, &into) {
                    Ok(made) if &made == path => {}
                    Ok(made) => {
                        moved.push((path.clone(), made.clone()));
                        done.made.push(made);
                    }
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
            done.undo = (!moved.is_empty()).then_some(Undo::Moved(moved));
        }
        Job::Trash(paths) => {
            let mut gone = Vec::new();
            for path in &paths {
                match trash_in(root, path) {
                    Ok(trashed) => gone.push(trashed),
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
            done.undo = (!gone.is_empty()).then_some(Undo::Trashed(gone));
        }
        Job::Delete(paths) => {
            for path in &paths {
                if let Err(error) = remove(path) {
                    done.failed.push(fail(path, error));
                }
                if let Some(trashed) = trashed_from(root, path) {
                    let _ = std::fs::remove_file(trashed.info);
                }
            }
        }
        Job::Restore(paths) => {
            for path in &paths {
                match trashed_from(root, path).ok_or_else(|| io::Error::other(crate::lang::t("Its first place is not known"))).and_then(|t| put_back(&t)) {
                    Ok(made) => done.made.push(made),
                    Err(error) => done.failed.push(fail(path, error)),
                }
            }
        }
        Job::EmptyTrash => {
            if let Err(error) = empty_trash_in(root) {
                done.failed.push(fail(root, error));
            }
        }
        Job::Undo(undo) => match undo {
            Undo::Made(paths) => {
                for path in &paths {
                    if let Err(error) = trash_in(root, path) {
                        done.failed.push(fail(path, error));
                    }
                }
            }
            Undo::Moved(pairs) => {
                for (was, now) in &pairs {
                    let back = was.parent().map(|parent| unique(parent, &crate::explorer::entry::name_of(was)));
                    match back {
                        Some(back) => {
                            if let Err(error) = move_to(now, &back) {
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
                    match put_back(trashed) {
                        Ok(made) => done.made.push(made),
                        Err(error) => done.failed.push(fail(&trashed.file, error)),
                    }
                }
            }
        },
    }
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
        let done = run_with(&place.trash(), Job::Trash(vec![file.clone()]));
        assert!(done.failed.is_empty(), "{:?}", done.failed);
        assert!(!file.exists());
        let info = std::fs::read_to_string(place.trash().join("info/old file.txt.trashinfo")).unwrap();
        assert!(info.contains("old%20file.txt"), "{info}");
        let found = trashed_from(&place.trash(), &place.trash().join("files/old file.txt")).unwrap();
        assert_eq!(found.from, std::path::absolute(&file).unwrap());
        let undone = run_with(&place.trash(), Job::Undo(done.undo.unwrap()));
        assert!(undone.failed.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
        assert!(!place.trash().join("info/old file.txt.trashinfo").exists());
    }

    #[test]
    fn two_trashed_files_with_one_name_both_stay() {
        let place = Place::new("twice");
        for _ in 0..2 {
            std::fs::write(place.work().join("a.txt"), "x").unwrap();
            run_with(&place.trash(), Job::Trash(vec![place.work().join("a.txt")]));
        }
        assert!(place.trash().join("files/a.txt").exists());
        assert!(place.trash().join("files/a.2.txt").exists());
        run_with(&place.trash(), Job::EmptyTrash);
        assert_eq!(std::fs::read_dir(place.trash().join("files")).unwrap().count(), 0);
    }

    #[test]
    fn a_move_can_be_undone() {
        let place = Place::new("move");
        let file = place.work().join("m.txt");
        std::fs::write(&file, "x").unwrap();
        let into = place.work().join("into");
        std::fs::create_dir(&into).unwrap();
        let done = run_with(&place.trash(), Job::Move { from: vec![file.clone()], into: into.clone() });
        assert!(into.join("m.txt").exists() && !file.exists());
        run_with(&place.trash(), Job::Undo(done.undo.unwrap()));
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
