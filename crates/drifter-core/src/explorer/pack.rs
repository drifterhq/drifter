use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::entry::name_of;
use super::ops::{Progress, unique};
use super::prefs::on_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Tar,
    Zip,
    Seven,
    Rar,
    Single(&'static str),
}

const ENDINGS: [(&str, Kind); 20] = [
    (".tar.gz", Kind::Tar),
    (".tar.xz", Kind::Tar),
    (".tar.bz2", Kind::Tar),
    (".tar.zst", Kind::Tar),
    (".tar.lz", Kind::Tar),
    (".tgz", Kind::Tar),
    (".txz", Kind::Tar),
    (".tbz", Kind::Tar),
    (".tbz2", Kind::Tar),
    (".tzst", Kind::Tar),
    (".tar", Kind::Tar),
    (".zip", Kind::Zip),
    (".jar", Kind::Zip),
    (".7z", Kind::Seven),
    (".rar", Kind::Rar),
    (".gz", Kind::Single("gzip")),
    (".xz", Kind::Single("xz")),
    (".bz2", Kind::Single("bzip2")),
    (".zst", Kind::Single("zstd")),
    (".lz", Kind::Single("lzip")),
];

pub fn kind_of(name: &str) -> Option<(Kind, &str)> {
    let lower = name.to_lowercase();
    ENDINGS.iter().find(|(ending, _)| lower.ends_with(ending) && lower.len() > ending.len()).map(|(ending, kind)| (*kind, &name[..name.len() - ending.len()]))
}

pub fn can_extract(path: &Path) -> bool {
    path.is_file() && kind_of(&name_of(path)).is_some()
}

fn missing() -> io::Error {
    io::Error::other(crate::lang::t("Install libarchive-tools to open this kind of archive"))
}

fn run(command: &mut Command, progress: &Progress, output: Option<&Path>) -> io::Result<()> {
    command.stdin(Stdio::null()).stderr(Stdio::piped());
    match output {
        Some(path) => {
            command.stdout(std::fs::OpenOptions::new().write(true).create_new(true).open(path)?);
        }
        None => {
            command.stdout(Stdio::null());
        }
    }
    let mut child = command.spawn()?;
    let mut said = child.stderr.take();
    let words = std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        if let Some(said) = said.as_mut() {
            let _ = said.read_to_string(&mut text);
        }
        text
    });
    let status = loop {
        if progress.stopped() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::Interrupted, crate::lang::t("Stopped")));
        }
        match child.try_wait()? {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(40)),
        }
    };
    let words = words.join().unwrap_or_default();
    match status.success() {
        true => Ok(()),
        false => {
            let last = words.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("").trim().to_string();
            Err(io::Error::other(match last.is_empty() {
                true => status.to_string(),
                false => last,
            }))
        }
    }
}

fn unpack(archive: &Path, kind: Kind, into: &Path, progress: &Progress) -> io::Result<()> {
    let bsdtar = on_path("bsdtar");
    let mut command = match kind {
        Kind::Tar | Kind::Zip | Kind::Seven | Kind::Rar if bsdtar => {
            let mut command = Command::new("bsdtar");
            command.arg("-x").arg("-f").arg(archive).arg("-C").arg(into);
            command
        }
        Kind::Tar if on_path("tar") => {
            let mut command = Command::new("tar");
            command.arg("-x").arg("-f").arg(archive).arg("-C").arg(into);
            command
        }
        Kind::Zip if on_path("unzip") => {
            let mut command = Command::new("unzip");
            command.arg("-q").arg(archive).arg("-d").arg(into);
            command
        }
        Kind::Seven if on_path("7z") => {
            let mut command = Command::new("7z");
            command.arg("x").arg("-y").arg(format!("-o{}", into.display())).arg(archive);
            command
        }
        Kind::Rar if on_path("unrar") => {
            let mut command = Command::new("unrar");
            command.arg("x").arg("-o-").arg(archive).arg(into);
            command
        }
        _ => return Err(missing()),
    };
    run(&mut command, progress, None)
}

pub fn extract(archive: &Path, into: &Path, progress: &Progress) -> io::Result<PathBuf> {
    let name = name_of(archive);
    let (kind, stem) = kind_of(&name).ok_or_else(|| io::Error::other(crate::lang::t("This is not an archive Explorer can open")))?;
    let stem = if stem.is_empty() { name.as_str() } else { stem };
    if let Kind::Single(tool) = kind {
        if !on_path(tool) {
            return Err(io::Error::other(crate::lang::tf("Install {} to open this file", &[&tool])));
        }
        let target = unique(into, stem);
        let result = run(Command::new(tool).arg("-d").arg("-c").arg(archive), progress, Some(&target));
        if result.is_err() {
            let _ = std::fs::remove_file(&target);
        }
        return result.map(|()| target);
    }
    let work = unique(into, &format!(".{stem}.opening"));
    std::fs::create_dir(&work)?;
    let placed = unpack(archive, kind, &work, progress).and_then(|()| {
        let inside: Vec<PathBuf> = std::fs::read_dir(&work)?.flatten().map(|item| item.path()).collect();
        match inside.as_slice() {
            [one] => {
                let target = unique(into, &name_of(one));
                std::fs::rename(one, &target)?;
                std::fs::remove_dir(&work)?;
                Ok(target)
            }
            _ => {
                let target = unique(into, stem);
                std::fs::rename(&work, &target)?;
                Ok(target)
            }
        }
    });
    if placed.is_err() && work.exists() {
        let _ = std::fs::remove_dir_all(&work);
    }
    placed
}

pub fn compress(from: &[PathBuf], progress: &Progress) -> io::Result<PathBuf> {
    let Some(parent) = from.first().and_then(|first| first.parent()).map(Path::to_path_buf) else {
        return Err(io::Error::other(crate::lang::t("Nothing to compress")));
    };
    if from.iter().any(|path| path.parent() != Some(parent.as_path())) {
        return Err(io::Error::other(crate::lang::t("Items to compress must be in one folder")));
    }
    let name = match from {
        [one] => format!("{}.zip", name_of(one)),
        _ => format!("{}.zip", crate::lang::t("Archive")),
    };
    let target = unique(&parent, &name);
    let names: Vec<String> = from.iter().map(|path| name_of(path)).collect();
    let mut command = match (on_path("bsdtar"), on_path("zip")) {
        (true, _) => {
            let mut command = Command::new("bsdtar");
            command.arg("--format").arg("zip").arg("-c").arg("-f").arg(&target).arg("--").args(&names);
            command
        }
        (false, true) => {
            let mut command = Command::new("zip");
            command.arg("-r").arg("-q").arg("-y").arg(&target).arg("--").args(&names);
            command
        }
        (false, false) => return Err(missing()),
    };
    command.current_dir(&parent);
    let result = run(&mut command, progress, None);
    if result.is_err() {
        let _ = std::fs::remove_file(&target);
    }
    result.map(|()| target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_names_split_into_kind_and_stem() {
        assert_eq!(kind_of("photos.tar.gz"), Some((Kind::Tar, "photos")));
        assert_eq!(kind_of("Photos.ZIP"), Some((Kind::Zip, "Photos")));
        assert_eq!(kind_of("notes.txt.gz"), Some((Kind::Single("gzip"), "notes.txt")));
        assert_eq!(kind_of("a.7z"), Some((Kind::Seven, "a")));
        assert_eq!(kind_of(".zip"), None);
        assert_eq!(kind_of("song.flac"), None);
    }

    #[test]
    fn compress_then_extract_round_trips() {
        if !on_path("bsdtar") && !(on_path("zip") && on_path("unzip")) {
            return;
        }
        let dir = std::env::temp_dir().join(format!("drifter-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Photos/2026")).unwrap();
        std::fs::write(dir.join("Photos/2026/cat.txt"), "meow").unwrap();
        std::fs::write(dir.join("-dash.txt"), "dash").unwrap();
        let progress = Progress::default();
        let one = compress(&[dir.join("Photos")], &progress).unwrap();
        assert_eq!(one, dir.join("Photos.zip"));
        let many = compress(&[dir.join("Photos"), dir.join("-dash.txt")], &progress).unwrap();
        assert_eq!(name_of(&many), format!("{}.zip", crate::lang::t("Archive")));
        let out = dir.join("out");
        std::fs::create_dir(&out).unwrap();
        let single = extract(&one, &out, &progress).unwrap();
        assert_eq!(single, out.join("Photos"));
        assert_eq!(std::fs::read_to_string(out.join("Photos/2026/cat.txt")).unwrap(), "meow");
        let again = extract(&one, &out, &progress).unwrap();
        assert_eq!(again, out.join("Photos 2"));
        let both = extract(&many, &out, &progress).unwrap();
        assert_eq!(both, out.join(crate::lang::t("Archive")));
        assert_eq!(std::fs::read_to_string(both.join("-dash.txt")).unwrap(), "dash");
        let broken = dir.join("broken.zip");
        std::fs::write(&broken, "not a zip").unwrap();
        assert!(extract(&broken, &out, &progress).is_err());
        assert!(std::fs::read_dir(&out).unwrap().flatten().all(|item| !item.file_name().to_string_lossy().starts_with('.')));
        if on_path("gzip") {
            assert!(Command::new("gzip").arg("-k").arg("--").arg("-dash.txt").current_dir(&dir).status().unwrap().success());
            let plain = extract(&dir.join("-dash.txt.gz"), &out, &progress).unwrap();
            assert_eq!(std::fs::read_to_string(plain).unwrap(), "dash");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
