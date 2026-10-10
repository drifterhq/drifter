use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub type Changed = Box<dyn Fn(Vec<PathBuf>) + Send>;

pub struct Watcher {
    wants: Option<(Sender<Vec<PathBuf>>, Bell)>,
    last: Vec<PathBuf>,
}

impl Watcher {
    pub fn start(changed: Changed) -> Watcher {
        Watcher { wants: spawn(changed), last: Vec::new() }
    }

    pub fn on(&self) -> bool {
        self.wants.is_some()
    }

    pub fn watch(&mut self, mut dirs: Vec<PathBuf>) {
        dirs.sort();
        dirs.dedup();
        if dirs == self.last {
            return;
        }
        self.last = dirs.clone();
        if let Some((wants, bell)) = &self.wants {
            match wants.send(dirs) {
                Ok(()) => bell.ring(),
                Err(_) => self.wants = None,
            }
        }
    }
}

#[cfg(target_os = "linux")]
struct Bell(i32);

#[cfg(target_os = "linux")]
impl Bell {
    fn ring(&self) {
        let one = 1u8;
        unsafe { libc::write(self.0, (&raw const one).cast(), 1) };
    }
}

#[cfg(target_os = "linux")]
impl Drop for Bell {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

#[cfg(not(target_os = "linux"))]
struct Bell;

#[cfg(not(target_os = "linux"))]
impl Bell {
    fn ring(&self) {}
}

#[cfg(target_os = "linux")]
fn spawn(changed: Changed) -> Option<(Sender<Vec<PathBuf>>, Bell)> {
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
    if fd < 0 {
        return None;
    }
    let mut ends = [0i32; 2];
    if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
        unsafe { libc::close(fd) };
        return None;
    }
    let (wants, asked) = std::sync::mpsc::channel();
    let heard = ends[0];
    let spawned = std::thread::Builder::new().name("explorer-watch".to_string()).spawn(move || linux::serve(fd, heard, &asked, &changed));
    match spawned {
        Ok(_) => Some((wants, Bell(ends[1]))),
        Err(_) => {
            unsafe {
                libc::close(fd);
                libc::close(ends[0]);
                libc::close(ends[1]);
            }
            None
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn spawn(_: Changed) -> Option<(Sender<Vec<PathBuf>>, Bell)> {
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::{HashMap, HashSet};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::mpsc::{Receiver, TryRecvError};
    use std::time::{Duration, Instant};

    const SETTLE: Duration = Duration::from_millis(120);
    const MASK: u32 = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_ATTRIB | libc::IN_CLOSE_WRITE | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_ONLYDIR;

    pub fn serve(fd: i32, bell: i32, asked: &Receiver<Vec<PathBuf>>, changed: &super::Changed) {
        let mut by_wd: HashMap<i32, PathBuf> = HashMap::new();
        let mut dirty: HashSet<PathBuf> = HashSet::new();
        let mut since: Option<Instant> = None;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let mut latest = None;
            loop {
                match asked.try_recv() {
                    Ok(dirs) => latest = Some(dirs),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        unsafe {
                            libc::close(fd);
                            libc::close(bell);
                        }
                        return;
                    }
                }
            }
            if let Some(dirs) = latest {
                let wanted: HashSet<PathBuf> = dirs.into_iter().collect();
                by_wd.retain(|wd, dir| {
                    let keep = wanted.contains(dir);
                    if !keep {
                        unsafe { libc::inotify_rm_watch(fd, *wd) };
                    }
                    keep
                });
                let have: HashSet<PathBuf> = by_wd.values().cloned().collect();
                for dir in wanted.difference(&have) {
                    let Ok(name) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
                        continue;
                    };
                    let wd = unsafe { libc::inotify_add_watch(fd, name.as_ptr(), MASK) };
                    if wd >= 0 {
                        by_wd.insert(wd, dir.clone());
                    }
                }
            }
            let wait = match since {
                Some(at) => SETTLE.saturating_sub(at.elapsed()).as_millis() as i32 + 1,
                None => -1,
            };
            let mut polls = [libc::pollfd { fd, events: libc::POLLIN, revents: 0 }, libc::pollfd { fd: bell, events: libc::POLLIN, revents: 0 }];
            let ready = unsafe { libc::poll(polls.as_mut_ptr(), 2, wait) };
            if ready > 0 && polls[1].revents != 0 {
                let mut drain = [0u8; 64];
                while unsafe { libc::read(bell, drain.as_mut_ptr().cast(), drain.len()) } > 0 {}
            }
            if ready > 0 && polls[1].revents & libc::POLLHUP != 0 && polls[0].revents == 0 {
                continue;
            }
            if ready > 0 && polls[0].revents != 0 {
                loop {
                    let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                    if read <= 0 {
                        break;
                    }
                    let mut at = 0usize;
                    let head = std::mem::size_of::<libc::inotify_event>();
                    while at + head <= read as usize {
                        let event: libc::inotify_event = unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(at).cast()) };
                        if event.mask & libc::IN_Q_OVERFLOW != 0 {
                            dirty.extend(by_wd.values().cloned());
                        } else if let Some(dir) = by_wd.get(&event.wd) {
                            dirty.insert(dir.clone());
                            if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF) != 0
                                && let Some(parent) = dir.parent()
                            {
                                dirty.insert(parent.to_path_buf());
                            }
                        }
                        if event.mask & libc::IN_IGNORED != 0 {
                            by_wd.remove(&event.wd);
                        }
                        at += head + event.len as usize;
                    }
                }
                if !dirty.is_empty() && since.is_none() {
                    since = Some(Instant::now());
                }
            }
            if since.is_some_and(|at| at.elapsed() >= SETTLE) {
                since = None;
                changed(dirty.drain().collect());
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_new_file_wakes_the_watcher() {
        let dir = std::env::temp_dir().join(format!("drifter-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (told, heard) = std::sync::mpsc::channel();
        let mut watcher = Watcher::start(Box::new(move |dirs| {
            let _ = told.send(dirs);
        }));
        assert!(watcher.on());
        watcher.watch(vec![dir.clone()]);
        std::thread::sleep(std::time::Duration::from_millis(200));
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let dirs = heard.recv_timeout(std::time::Duration::from_secs(3)).unwrap();
        assert_eq!(dirs, vec![dir.clone()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
