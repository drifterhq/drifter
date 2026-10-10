use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Instant, SystemTime};

use super::entry::{self, Entry, Sort};

pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lens {
    pub hidden: bool,
    pub sort: Sort,
    pub down: bool,
    pub folders_first: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ask {
    List(PathBuf, Lens),
    Folders(PathBuf, bool),
    Stamp(PathBuf),
    Peek(PathBuf),
    Drives,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub lens: Lens,
    pub seq: u64,
    pub entries: Result<Vec<Entry>, String>,
    pub stamp: Option<SystemTime>,
    pub free: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Got {
    List(PathBuf, Listing),
    Folders(PathBuf, bool, Vec<PathBuf>),
    Stamp(PathBuf, Option<SystemTime>),
    Peek(PathBuf, Option<Vec<String>>),
    Changed(Vec<PathBuf>),
    Drives(Vec<super::drives::Drive>),
}

pub struct Reader {
    done: Sender<(Option<Ask>, Got)>,
    back: Receiver<(Option<Ask>, Got)>,
    pending: HashMap<Ask, u64>,
    next: u64,
    wake: Arc<Mutex<Option<Wake>>>,
    watcher: Option<super::watch::Watcher>,
}

impl Default for Reader {
    fn default() -> Reader {
        let (done, back) = channel();
        Reader { done, back, pending: HashMap::new(), next: 0, wake: Arc::default(), watcher: None }
    }
}

fn woken(wake: &Mutex<Option<Wake>>) {
    let wake = wake.lock().ok().and_then(|slot| slot.clone());
    if let Some(wake) = wake {
        wake();
    }
}

impl Reader {
    pub fn set_wake(&mut self, wake: Wake) {
        if let Ok(mut slot) = self.wake.lock() {
            *slot = Some(wake);
        }
    }

    pub fn ask(&mut self, ask: Ask) -> u64 {
        if let Some(seq) = self.pending.get(&ask) {
            return *seq;
        }
        self.next += 1;
        let seq = self.next;
        self.pending.insert(ask.clone(), seq);
        let (done, wake) = (self.done.clone(), self.wake.clone());
        let work = ask.clone();
        let spawned = std::thread::Builder::new().name("explorer-read".to_string()).spawn(move || {
            let got = run(&work, seq);
            let _ = done.send((Some(work), got));
            woken(&wake);
        });
        if spawned.is_err() {
            let _ = self.done.send((Some(ask.clone()), run(&ask, seq)));
        }
        seq
    }

    pub fn watch(&mut self, dirs: Vec<PathBuf>) {
        let watcher = self.watcher.get_or_insert_with(|| {
            let (done, wake) = (self.done.clone(), self.wake.clone());
            super::watch::Watcher::start(Box::new(move |dirs| {
                let _ = done.send((None, Got::Changed(dirs)));
                woken(&wake);
            }))
        });
        watcher.watch(dirs);
    }

    pub fn watching(&self) -> bool {
        self.watcher.as_ref().is_some_and(super::watch::Watcher::on)
    }

    pub fn asked(&self, ask: &Ask) -> bool {
        self.pending.contains_key(ask)
    }

    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn next(&mut self, until: Option<Instant>) -> Option<Got> {
        let (ask, got) = match until {
            None => self.back.try_recv().ok()?,
            Some(at) => self.back.recv_timeout(at.saturating_duration_since(Instant::now())).ok()?,
        };
        if let Some(ask) = ask {
            self.pending.remove(&ask);
        }
        Some(got)
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn run(ask: &Ask, seq: u64) -> Got {
    match ask {
        Ask::List(dir, lens) => {
            let stamp = modified(dir);
            let trash = *dir == super::ops::trash_files();
            let read = match entry::read(dir, lens.hidden) {
                Err(error) if trash && error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                other => other,
            };
            let entries = read.map_err(|error| error.to_string()).map(|mut found| {
                if trash {
                    for other in super::ops::other_trashes() {
                        found.extend(entry::read(&other.join("files"), lens.hidden).unwrap_or_default());
                    }
                    for each in &mut found {
                        let root = super::ops::root_of(&each.path).unwrap_or_else(super::ops::trash_root);
                        each.trashed = super::ops::trashed_from(&root, &each.path).map(|trashed| {
                            let when = super::ops::deleted_at(&trashed.info);
                            (trashed.from, when)
                        });
                    }
                }
                entry::sort(&mut found, lens.sort, lens.down, lens.folders_first);
                found
            });
            let free = entry::free_space(dir);
            Got::List(dir.clone(), Listing { lens: *lens, seq, entries, stamp, free })
        }
        Ask::Folders(dir, hidden) => Got::Folders(dir.clone(), *hidden, entry::folders_in(dir, *hidden)),
        Ask::Stamp(dir) => Got::Stamp(dir.clone(), modified(dir)),
        Ask::Peek(path) => Got::Peek(path.clone(), peek(path)),
        Ask::Drives => Got::Drives(super::drives::list()),
    }
}

pub fn peek(path: &Path) -> Option<Vec<String>> {
    use std::io::Read;
    let mut bytes = vec![0u8; 16 * 1024];
    let read = std::fs::File::open(path).ok()?.read(&mut bytes).ok()?;
    bytes.truncate(read);
    if bytes.contains(&0) {
        return None;
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_string(),
        Err(error) if error.valid_up_to() > bytes.len().saturating_sub(4) => String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned(),
        Err(_) => return None,
    };
    Some(text.lines().take(80).map(|line| line.replace('\t', "    ")).collect())
}
