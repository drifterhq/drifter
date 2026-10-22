use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use drifter_core::spatial::{self, Stream};

const EVERY: Duration = Duration::from_millis(100);
const RECHECK: Duration = Duration::from_secs(1);
const IDLE: Duration = Duration::from_secs(3600);
const ANCESTORS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Seat {
    pub pid: u32,
    pub app: String,
    pub pan: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub on: bool,
    pub mix: f64,
    pub ignore: Vec<String>,
    pub seats: Vec<Seat>,
}

pub struct Spatial {
    tx: Sender<Scene>,
    last: Option<Scene>,
}

impl Spatial {
    pub fn start() -> Option<Spatial> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("drifter-spatial".into())
            .spawn(move || run(rx))
            .ok()?;
        Some(Spatial { tx, last: None })
    }

    pub fn tell(&mut self, scene: Scene) {
        if self.last.as_ref() == Some(&scene) {
            return;
        }
        if self.tx.send(scene.clone()).is_ok() {
            self.last = Some(scene);
        }
    }
}

fn run(rx: Receiver<Scene>) {
    let mut scene: Option<Scene> = None;
    let mut sent: HashMap<u32, (f64, f64)> = HashMap::new();
    let mut applied: Option<Instant> = None;
    loop {
        let wait = match &scene {
            Some(held) if held.on || !sent.is_empty() => RECHECK,
            _ => IDLE,
        };
        match rx.recv_timeout(wait) {
            Ok(fresh) => scene = Some(fresh),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if let Some(at) = applied {
            let gone = at.elapsed();
            if gone < EVERY {
                std::thread::sleep(EVERY - gone);
            }
        }
        while let Ok(fresh) = rx.try_recv() {
            scene = Some(fresh);
        }
        if let Some(held) = &scene {
            apply(held, &mut sent);
            applied = Some(Instant::now());
        }
    }
}

fn apply(scene: &Scene, sent: &mut HashMap<u32, (f64, f64)>) {
    if !scene.on && sent.is_empty() {
        return;
    }
    let Some(list) = pactl(&["list", "sink-inputs"]) else {
        return;
    };
    let streams = spatial::read_streams(&list);
    sent.retain(|index, _| streams.iter().any(|stream| stream.index == *index));
    for stream in streams.iter().filter(|stream| stream.pans()) {
        let wanted = match scene.on {
            true => aim(scene, stream),
            false => (1.0, 1.0),
        };
        let even = !spatial::moved((1.0, 1.0), wanted);
        if even && !sent.contains_key(&stream.index) {
            continue;
        }
        let was = sent.get(&stream.index).copied().unwrap_or_else(|| stream.balance());
        if !spatial::moved(was, wanted) {
            continue;
        }
        let volumes: Vec<String> = stream.volumes(wanted).iter().map(u32::to_string).collect();
        let mut args = vec!["set-sink-input-volume".to_string(), stream.index.to_string()];
        args.extend(volumes);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if pactl(&args).is_none() {
            continue;
        }
        match even {
            true => {
                sent.remove(&stream.index);
            }
            false => {
                sent.insert(stream.index, wanted);
            }
        }
    }
}

fn aim(scene: &Scene, stream: &Stream) -> (f64, f64) {
    let seat = stream.pid.and_then(|pid| seat_of(scene, pid));
    let app = seat.map_or("", |seat| seat.app.as_str());
    if spatial::ignored(&scene.ignore, app, &stream.binary, &stream.name) {
        return (1.0, 1.0);
    }
    match seat {
        Some(seat) => spatial::gains(seat.pan, scene.mix),
        None => (1.0, 1.0),
    }
}

fn seat_of(scene: &Scene, pid: u32) -> Option<&Seat> {
    let mut at = pid;
    for _ in 0..ANCESTORS {
        if let Some(seat) = scene.seats.iter().rev().find(|seat| seat.pid == at) {
            return Some(seat);
        }
        let stat = std::fs::read_to_string(format!("/proc/{at}/stat")).ok()?;
        at = spatial::parent(&stat).filter(|up| *up > 1)?;
    }
    None
}

fn pactl(args: &[&str]) -> Option<String> {
    Command::new("pactl")
        .env("LC_ALL", "C")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}
