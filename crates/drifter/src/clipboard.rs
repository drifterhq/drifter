use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::input::Seat;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::channel::{Channel, Sender, channel};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::wayland::selection::data_device::{
    request_data_device_client_selection, set_data_device_selection,
};

use crate::render::State;

pub type Clip = Arc<Vec<(String, Vec<u8>)>>;

const MOST: usize = 32 << 20;
const PATIENCE: Duration = Duration::from_secs(2);

pub struct Copied {
    generation: u64,
    data: Vec<(String, Vec<u8>)>,
}

pub struct Keeper {
    generation: u64,
    handle: LoopHandle<'static, State>,
    tx: Sender<Copied>,
}

impl Keeper {
    pub fn new(handle: LoopHandle<'static, State>) -> (Keeper, Channel<Copied>) {
        let (tx, rx) = channel();
        (
            Keeper {
                generation: 0,
                handle,
                tx,
            },
            rx,
        )
    }

    pub fn changed(&mut self, offered: Option<Vec<String>>) {
        self.generation += 1;
        let Some(mimes) = offered else {
            return;
        };
        let generation = self.generation;
        self.handle.insert_idle(move |state: &mut State| {
            let seat = state.wayland._seat.clone();
            state.clip.copy(generation, mimes, &seat);
        });
    }

    fn copy(&self, generation: u64, mut mimes: Vec<String>, seat: &Seat<State>) {
        if generation != self.generation {
            return;
        }
        mimes.dedup();
        let mut pipes = Vec::new();
        for mime in mimes {
            let Ok((reader, writer)) = std::io::pipe() else {
                continue;
            };
            if request_data_device_client_selection(seat, mime.clone(), OwnedFd::from(writer))
                .is_ok()
            {
                pipes.push((mime, reader));
            }
        }
        if pipes.is_empty() {
            return;
        }
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-clipboard".to_string())
            .spawn(move || {
                if let Some(data) = read_all(pipes) {
                    let _ = tx.send(Copied { generation, data });
                }
            });
        if let Err(error) = spawned {
            eprintln!("drifter: cannot keep the clipboard: {error}");
        }
    }

    pub fn arrived(&self, copied: Copied, dh: &DisplayHandle, seat: &Seat<State>) {
        if copied.generation != self.generation || copied.data.is_empty() {
            return;
        }
        let mimes = copied.data.iter().map(|(mime, _)| mime.clone()).collect();
        set_data_device_selection(dh, seat, mimes, Arc::new(copied.data));
    }
}

fn read_all(pipes: Vec<(String, std::io::PipeReader)>) -> Option<Vec<(String, Vec<u8>)>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let count = pipes.len();
    for (index, (mime, mut reader)) in pipes.into_iter().enumerate() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = (&mut reader).take(MOST as u64 + 1).read_to_end(&mut bytes);
            let _ = tx.send((index, mime, read.ok().map(|_| bytes)));
        });
    }
    let deadline = Instant::now() + PATIENCE;
    let mut found = Vec::new();
    let mut total = 0;
    for _ in 0..count {
        let wait = deadline.saturating_duration_since(Instant::now());
        let (index, mime, bytes) = rx.recv_timeout(wait).ok()?;
        let bytes = bytes?;
        total += bytes.len();
        if total > MOST {
            return None;
        }
        found.push((index, mime, bytes));
    }
    found.sort_by_key(|(index, _, _)| *index);
    Some(
        found
            .into_iter()
            .map(|(_, mime, bytes)| (mime, bytes))
            .collect(),
    )
}

pub fn send(clip: &Clip, mime: &str, fd: OwnedFd) {
    let clip = clip.clone();
    let mime = mime.to_string();
    std::thread::spawn(move || {
        if let Some((_, bytes)) = clip.iter().find(|(offered, _)| *offered == mime) {
            let _ = std::fs::File::from(fd).write_all(bytes);
        }
    });
}
