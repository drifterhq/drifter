use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use super::*;
use drifter_core::airdrop::{self, Heard, Report, Sent};
use drifter_core::blue::{Ask, Kind, Question};
use drifter_core::share::{self, Press, State as Status, Target};

const BUBBLE_IN: f64 = 0.42;
const BUBBLE_FROM: f64 = 0.55;
const BUBBLE_STAGGER: f64 = 0.06;
const SHAKE: f64 = 0.5;
const POP: f64 = 0.42;
const SHEET_IN: f64 = 0.34;
const SHEET_FROM: f64 = 0.94;
const PROMPT: &str = "accept? [y/N] ";

pub(super) struct Bubble {
    pub target: Target,
    pub born: Instant,
    pub art: Option<TextureBuffer<Tex>>,
    pub painted: Option<Target>,
    pub ring: Option<(TextureBuffer<Tex>, u8)>,
    pub changed: Instant,
    send: Option<Child>,
}

pub(super) struct Share {
    pub files: Vec<PathBuf>,
    pub bubbles: Vec<Bubble>,
    pub art: Option<(TextureBuffer<Tex>, Rect)>,
    pub painted: Option<(usize, String, bool)>,
    pub lane: worker::Lane,
    pub check: Option<TextureBuffer<Tex>>,
    pub haze: Option<TextureBuffer<Tex>>,
    pub status: Option<String>,
    browse: Option<Child>,
}

pub(super) struct Drop {
    pub shared: Arc<Mutex<crate::airdrop::Shared>>,
    pub report: Option<Report>,
    pub force: bool,
    pub folder: Option<String>,
    checking: bool,
    radio: Option<Child>,
    pub up: bool,
    pub until: Option<Instant>,
    receiver: Option<(Child, Option<ChildStdin>)>,
    pub peers: Vec<String>,
    session: u64,
    from: String,
    offered: Vec<(String, Option<u64>)>,
    asking: Option<u64>,
    received: Vec<PathBuf>,
    note: Option<(u32, Instant)>,
    settle: Option<Instant>,
    pub share: Option<Share>,
}

impl Default for Drop {
    fn default() -> Drop {
        Drop {
            shared: Arc::new(Mutex::new(crate::airdrop::Shared::default())),
            report: None,
            force: false,
            folder: None,
            checking: false,
            radio: None,
            up: false,
            until: None,
            receiver: None,
            peers: Vec::new(),
            session: 0,
            from: String::new(),
            offered: Vec::new(),
            asking: None,
            received: Vec::new(),
            note: None,
            settle: None,
            share: None,
        }
    }
}

fn spawn(args: &[&str], stdin: bool) -> Option<Child> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(airdrop::owly_program()?);
    unsafe {
        command.pre_exec(|| {
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::pthread_sigmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    command
        .args(args)
        .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| log!("drifter: cannot start owly: {error}"))
        .ok()
}

fn stop(child: &mut Child) {
    unsafe {
        libc::kill(child.id() as i32, libc::SIGTERM);
    }
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(1500) {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_into(stream: impl Read + Send + 'static, chores: smithay::reexports::calloop::channel::Sender<Chore>, line: impl Fn(&mut State, String) + Send + Sync + 'static, gone: impl FnOnce(&mut State) + Send + 'static) {
    let line = Arc::new(line);
    let _ = std::thread::Builder::new().name("drifter-owly".into()).spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut held: Vec<u8> = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match reader.read(&mut byte) {
                Ok(1) => {
                    held.push(byte[0]);
                    let ends = byte[0] == b'\n' || byte[0] == b'\r' || held.ends_with(PROMPT.as_bytes());
                    if ends {
                        let text = String::from_utf8_lossy(&held).trim_end_matches(['\n', '\r']).to_string();
                        held.clear();
                        if !text.is_empty() {
                            let line = line.clone();
                            let _ = chores.send(Box::new(move |state: &mut State| line(state, text)));
                        }
                    }
                }
                _ => break,
            }
        }
        let _ = chores.send(Box::new(gone));
    });
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Drifter".to_string())
}

impl State {
    pub(super) fn start_airdrop(&mut self) {
        if let Ok(config) = Config::load_from(&self.config_path) {
            self.drop.force = config.airdrop.force;
            self.drop.folder = config.airdrop.folder.clone();
        }
        match crate::airdrop::start(self.drop.shared.clone()) {
            Ok(heard) => {
                let inserted = self.handle.insert_source(heard, |event, _, state: &mut State| {
                    if let smithay::reexports::calloop::channel::Event::Msg(event) = event {
                        match event {
                            crate::airdrop::Event::Send(files) => state.airdrop_share(files),
                            crate::airdrop::Event::Receive(on) => state.airdrop_receive(on),
                            crate::airdrop::Event::Refresh => state.airdrop_refresh(),
                        }
                    }
                });
                if let Err(error) = inserted {
                    log!("drifter: cannot listen for AirDrop: {error}");
                }
            }
            Err(error) => log!("drifter: {error}"),
        }
        self.airdrop_refresh();
    }

    pub(super) fn airdrop_config(&mut self, config: &Config) {
        let changed = self.drop.force != config.airdrop.force;
        self.drop.force = config.airdrop.force;
        self.drop.folder = config.airdrop.folder.clone();
        self.airdrop_publish();
        if changed {
            self.airdrop_refresh();
        }
    }

    pub(super) fn airdrop_refresh(&mut self) {
        if self.drop.checking {
            return;
        }
        self.drop.checking = true;
        self.background("airdrop check", || {
            let report = drifter_blue::owly::report();
            move |state: &mut State| {
                state.drop.checking = false;
                state.drop.report = Some(report);
                state.airdrop_publish();
                if state.control.is_some() {
                    state.paint_control();
                }
            }
        });
    }

    pub(super) fn airdrop_shown(&self) -> bool {
        self.drop.report.as_ref().is_some_and(|report| report.shown(self.drop.force))
    }

    fn airdrop_ready(&self) -> bool {
        self.drop.report.as_ref().is_some_and(|report| report.owly && report.owlyd)
    }

    fn airdrop_publish(&mut self) {
        let shown = self.airdrop_shown();
        let ready = self.airdrop_ready();
        if let Ok(mut held) = self.drop.shared.lock() {
            *held = crate::airdrop::Shared {
                shown,
                ready,
                receiving: self.drop.until.is_some(),
            };
        }
        self.control_status.airdrop = shown.then(|| (self.drop.until.is_some(), self.airdrop_note()));
        self.blue_fill(drifter_core::control::Tile::AirDrop);
    }

    fn airdrop_note(&self) -> String {
        match (self.drop.until, self.airdrop_ready()) {
            (_, false) => drifter_core::lang::t("Needs setup").to_string(),
            (None, true) => drifter_core::lang::t("Off").to_string(),
            (Some(_), true) if !self.drop.up => drifter_core::lang::t("Turning on…").to_string(),
            (Some(until), true) => {
                let left = until.saturating_duration_since(Instant::now()).as_secs().div_ceil(60).max(1);
                drifter_core::lang::tf("Everyone, {} min", &[&left])
            }
        }
    }

    fn airdrop_say(&mut self, summary: &str, body: &str) {
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            log!("drifter: AirDrop: {summary}: {body}");
            return;
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: "AirDrop".to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions: Vec::new(),
            expire: None,
            asked: 8000,
            critical: false,
            icon: Some(airdrop::glyph(64, self.palette.ink)),
        });
    }

    pub(super) fn airdrop_receive(&mut self, on: bool) {
        if !on {
            self.drop.until = None;
            self.drop.settle = None;
            if let Some((mut child, _)) = self.drop.receiver.take() {
                stop(&mut child);
            }
            self.airdrop_release();
            self.airdrop_publish();
            self.paint_control();
            return;
        }
        if !self.airdrop_shown() {
            return;
        }
        if !self.airdrop_ready() {
            self.airdrop_say(drifter_core::lang::t("AirDrop needs setting up"), drifter_core::lang::t("Set it up in Settings, AirDrop."));
            self.open_settings_at("airdrop");
            return;
        }
        self.drop.until = Some(Instant::now() + Duration::from_secs(airdrop::RECEIVE_FOR));
        self.airdrop_radio();
        if self.drop.up {
            self.airdrop_listen();
        }
        self.airdrop_publish();
        self.paint_control();
    }

    fn airdrop_radio(&mut self) {
        if self.drop.radio.is_some() {
            return;
        }
        self.drop.session += 1;
        self.drop.up = false;
        self.drop.peers.clear();
        let session = self.drop.session;
        let Some(mut child) = spawn(&["radio", "up"], false) else {
            self.airdrop_failed(drifter_core::lang::t("AirDrop could not start.").to_string());
            return;
        };
        if let Some(err) = child.stderr.take() {
            read_into(
                err,
                self.chores.clone(),
                move |state, line| state.airdrop_radio_line(session, &line),
                move |state| state.airdrop_radio_gone(session),
            );
        }
        if let Some(out) = child.stdout.take() {
            read_into(out, self.chores.clone(), |_, _| {}, |_| {});
        }
        self.drop.radio = Some(child);
    }

    fn airdrop_radio_line(&mut self, session: u64, line: &str) {
        if session != self.drop.session {
            return;
        }
        if line.contains(" is up: ") {
            self.drop.up = true;
            if self.drop.until.is_some() {
                self.airdrop_listen();
            }
            if self.drop.share.is_some() {
                self.airdrop_browse();
            }
            self.airdrop_publish();
            self.paint_control();
            self.paint_share();
        } else if let Some(list) = line.strip_prefix("peers: ") {
            self.drop.peers = match list.trim() {
                "none" => Vec::new(),
                list => list
                    .split(", ")
                    .filter_map(|peer| peer.split_whitespace().next())
                    .map(|mac| mac.to_lowercase())
                    .collect(),
            };
            self.airdrop_peers();
        } else if let Some(reason) = line.strip_prefix("owly: ") {
            self.airdrop_failed(airdrop::plain_failure(reason));
        }
    }

    fn airdrop_radio_gone(&mut self, session: u64) {
        if session != self.drop.session {
            return;
        }
        let was = self.drop.up;
        self.drop.radio = None;
        self.drop.up = false;
        self.drop.peers.clear();
        if let Some((mut child, _)) = self.drop.receiver.take() {
            stop(&mut child);
        }
        if self.drop.until.take().is_some() && was {
            self.airdrop_say(drifter_core::lang::t("AirDrop is off"), drifter_core::lang::t("The Wi-Fi radio was given back."));
        }
        if let Some(share) = self.drop.share.as_mut()
            && share.status.is_none()
        {
            share.status = Some(drifter_core::lang::t("AirDrop stopped. Close this and try again.").to_string());
        }
        self.airdrop_publish();
        self.paint_control();
        self.paint_share();
    }

    fn airdrop_failed(&mut self, why: String) {
        self.drop.until = None;
        match self.drop.share.as_mut() {
            Some(share) => share.status = Some(why.clone()),
            None => self.airdrop_say(drifter_core::lang::t("AirDrop did not start"), &why),
        }
        self.airdrop_publish();
        self.paint_control();
        self.paint_share();
    }

    fn airdrop_release(&mut self) {
        if self.drop.until.is_some() || self.drop.share.is_some() {
            return;
        }
        if let Some(mut child) = self.drop.radio.take() {
            self.drop.session += 1;
            self.drop.up = false;
            self.drop.peers.clear();
            self.background("airdrop release", move || {
                stop(&mut child);
                |_: &mut State| {}
            });
        }
    }

    fn airdrop_listen(&mut self) {
        if self.drop.receiver.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = airdrop::received_folder(self.drop.folder.as_deref(), &home);
        let _ = std::fs::create_dir_all(&folder);
        let folder = folder.to_string_lossy().into_owned();
        let name = hostname();
        let session = self.drop.session;
        let Some(mut child) = spawn(&["receive", "--dir", &folder, "--name", &name], true) else {
            return;
        };
        if let Some(err) = child.stderr.take() {
            read_into(err, self.chores.clone(), move |state, line| state.airdrop_heard(session, &line), |_| {});
        }
        if let Some(out) = child.stdout.take() {
            read_into(out, self.chores.clone(), move |state, line| state.airdrop_got(session, &line), |_| {});
        }
        let stdin = child.stdin.take();
        self.drop.receiver = Some((child, stdin));
    }

    fn airdrop_heard(&mut self, session: u64, line: &str) {
        if session != self.drop.session {
            return;
        }
        match airdrop::read_receiver_line(line) {
            Some(Heard::Asking(from)) => {
                self.drop.from = from;
                self.drop.offered.clear();
            }
            Some(Heard::Offered(name, size)) => self.drop.offered.push((name, size)),
            Some(Heard::AskNow) => {
                let id = self.radio.next_ask;
                self.radio.next_ask += 1;
                self.drop.asking = Some(id);
                let ask = Ask {
                    id,
                    address: String::new(),
                    name: self.drop.from.clone(),
                    kind: Kind::Phone,
                    question: Question::Incoming(std::mem::take(&mut self.drop.offered)),
                };
                self.blue_queue_ask(ask);
            }
            Some(Heard::Withdrawn | Heard::TimedOut) => self.airdrop_drop_ask(),
            Some(Heard::Failed(reason)) => {
                self.airdrop_drop_ask();
                self.airdrop_say(drifter_core::lang::t("AirDrop did not finish"), &airdrop::plain_failure(&reason));
            }
            None => {}
        }
    }

    fn airdrop_drop_ask(&mut self) {
        if let Some(id) = self.drop.asking.take() {
            self.radio.asks.retain(|(ask, _)| ask.id != id);
            self.paint_pairing();
            self.kick_now();
        }
    }

    pub(super) fn airdrop_answer(&mut self, yes: bool) {
        self.drop.asking = None;
        if let Some((_, Some(stdin))) = self.drop.receiver.as_mut() {
            let _ = stdin.write_all(if yes { b"y\n" } else { b"n\n" });
            let _ = stdin.flush();
        }
        self.drop.received.clear();
    }

    fn airdrop_got(&mut self, session: u64, line: &str) {
        if session != self.drop.session {
            return;
        }
        let path = PathBuf::from(line.trim());
        if !path.is_absolute() {
            return;
        }
        self.drop.received.push(path);
        if self.airdrop_holds_wifi() {
            self.drop.settle = Some(Instant::now() + Duration::from_secs(4));
        }
        let from = self.drop.from.clone();
        let count = self.drop.received.len();
        let summary = match self.drop.received.as_slice() {
            [one] => drifter_core::lang::tf("Received {}", &[&one.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()]),
            _ => drifter_core::lang::tf("Received {} files", &[&count]),
        };
        let folder = self.drop.received[0].parent().map(|dir| dir.display().to_string()).unwrap_or_default();
        let body = drifter_core::lang::tf("From {}. Saved in {}.", &[&from, &folder]);
        let id = match self.drop.note {
            Some((id, at)) if at.elapsed() < Duration::from_secs(3) => Some(id),
            _ => self.notes_bus.as_ref().map(|bus| bus.next_id()),
        };
        if let Some(id) = id {
            self.drop.note = Some((id, Instant::now()));
            self.show_note(drifter_core::notify::Note {
                id,
                app: "AirDrop".to_string(),
                summary,
                body,
                actions: Vec::new(),
                expire: None,
                asked: 8000,
                critical: false,
                icon: Some(airdrop::glyph(64, self.palette.ink)),
            });
        }
    }

    pub(super) fn airdrop_holds_wifi(&self) -> bool {
        self.drop.radio.is_some() && self.drop.report.as_ref().is_some_and(|report| report.mode == Some(airdrop::Mode::Exclusive))
    }

    pub(super) fn airdrop_wind(&mut self) {
        let settled = self.drop.settle.is_some_and(|at| Instant::now() >= at);
        if settled || self.drop.until.is_some_and(|until| Instant::now() >= until) {
            self.airdrop_receive(false);
            return;
        }
        if self.drop.until.is_some() && self.control.is_some() {
            let note = self.airdrop_note();
            if self.control_status.airdrop.as_ref().is_some_and(|(_, held)| *held != note) {
                self.airdrop_publish();
                self.paint_control();
            }
        }
    }

    pub(super) fn airdrop_share(&mut self, files: Vec<PathBuf>) {
        if !self.airdrop_shown() || files.is_empty() {
            return;
        }
        self.airdrop_close_share();
        self.close_panels();
        self.drop.share = Some(Share {
            files,
            bubbles: Vec::new(),
            art: None,
            painted: None,
            lane: worker::Lane::default(),
            check: None,
            haze: None,
            status: (!self.airdrop_ready()).then(|| drifter_core::lang::t("Set it up in Settings, AirDrop.").to_string()),
            browse: None,
        });
        if self.airdrop_ready() {
            self.airdrop_radio();
            if self.drop.up {
                self.airdrop_browse();
            }
        }
        self.paint_share();
        self.kick_now();
    }

    fn airdrop_browse(&mut self) {
        let session = self.drop.session;
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        if share.browse.is_some() {
            return;
        }
        let Some(mut child) = spawn(&["browse", "--seconds", "3600"], false) else {
            return;
        };
        if let Some(out) = child.stdout.take() {
            read_into(out, self.chores.clone(), move |state, line| state.airdrop_found(session, &line), |_| {});
        }
        if let Some(err) = child.stderr.take() {
            read_into(err, self.chores.clone(), |_, _| {}, |_| {});
        }
        share.browse = Some(child);
        self.airdrop_peers();
    }

    pub(super) fn airdrop_add(&mut self, target: Target) {
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        let now = Instant::now();
        let fresh = share.bubbles.iter().filter(|bubble| now < bubble.born + Duration::from_secs_f64(BUBBLE_IN)).count();
        share.bubbles.push(Bubble {
            target,
            born: now + Duration::from_secs_f64(BUBBLE_STAGGER * fresh as f64),
            art: None,
            painted: None,
            ring: None,
            changed: now,
            send: None,
        });
        self.paint_share();
        self.kick_now();
    }

    fn airdrop_found(&mut self, session: u64, line: &str) {
        if session != self.drop.session {
            return;
        }
        let Some((instance, name, mac)) = airdrop::read_browse_line(line) else {
            return;
        };
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        if share.bubbles.iter().any(|bubble| bubble.target.key == instance) {
            return;
        }
        if let Some(mac) = mac.as_ref() {
            share.bubbles.retain(|bubble| !(bubble.target.mac.as_ref() == Some(mac) && bubble.target.state == Status::Idle && bubble.target.key == *mac));
        }
        let kind = Kind::guess("", None, None, &name);
        self.airdrop_add(Target {
            key: instance,
            kind: if kind == Kind::Other { Kind::Computer } else { kind },
            name,
            mac,
            state: Status::Idle,
        });
    }

    fn airdrop_peers(&mut self) {
        let peers = self.drop.peers.clone();
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        share.bubbles.retain(|bubble| {
            let unnamed = bubble.target.key == bubble.target.mac.clone().unwrap_or_default();
            !(unnamed && bubble.target.state == Status::Idle && !peers.contains(&bubble.target.key))
        });
        let fresh: Vec<String> = peers
            .into_iter()
            .filter(|mac| !share.bubbles.iter().any(|bubble| bubble.target.mac.as_deref() == Some(mac.as_str())))
            .collect();
        for mac in fresh {
            self.airdrop_add(Target {
                key: mac.clone(),
                name: drifter_core::lang::t("iPhone or iPad nearby").to_string(),
                kind: Kind::Phone,
                mac: Some(mac),
                state: Status::Idle,
            });
        }
        self.paint_share();
    }

    fn airdrop_send(&mut self, index: usize) {
        let session = self.drop.session;
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        let files: Vec<String> = share.files.iter().map(|file| file.to_string_lossy().into_owned()).collect();
        let Some(bubble) = share.bubbles.get_mut(index) else {
            return;
        };
        if matches!(bubble.target.state, Status::Waiting | Status::Sending(_)) {
            return;
        }
        let mut args: Vec<String> = vec!["send".into()];
        match (bubble.target.key == bubble.target.mac.clone().unwrap_or_default(), bubble.target.mac.as_ref()) {
            (true, Some(mac)) => args.extend(["--mac".into(), mac.clone()]),
            _ => args.extend(["--to".into(), bubble.target.name.clone()]),
        }
        args.extend(files);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(mut child) = spawn(&refs, false) else {
            return;
        };
        bubble.target.state = Status::Waiting;
        bubble.changed = Instant::now();
        let key = bubble.target.key.clone();
        for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)].into_iter().flatten() {
            let key = key.clone();
            let gone = key.clone();
            read_into(
                stream,
                self.chores.clone(),
                move |state, line| state.airdrop_sent(session, &key, &line),
                move |state| state.airdrop_send_gone(session, &gone),
            );
        }
        bubble.send = Some(child);
        share.status = None;
        self.paint_share();
        self.kick_now();
    }

    fn airdrop_sent(&mut self, session: u64, key: &str, line: &str) {
        if session != self.drop.session {
            return;
        }
        let mut status = None;
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        let Some(bubble) = share.bubbles.iter_mut().find(|bubble| bubble.target.key == key) else {
            return;
        };
        for heard in airdrop::read_sender_text(line) {
            let next = match heard {
                Sent::Accepted => Status::Sending(0),
                Sent::Progress(sent, total) => Status::Sending(((sent * 100) / total.max(1)).min(100) as u8),
                Sent::Delivered => Status::Sent,
                Sent::Declined => Status::Declined,
                Sent::Failed(reason) => {
                    status = Some(airdrop::plain_failure(&reason));
                    Status::Failed
                }
            };
            let settled = matches!(bubble.target.state, Status::Sent | Status::Declined | Status::Failed);
            let going = matches!(next, Status::Waiting | Status::Sending(_));
            if next != bubble.target.state && !(settled && going) {
                bubble.target.state = next;
                bubble.changed = Instant::now();
            }
        }
        if status.is_some() {
            share.status = status;
        }
        self.paint_share();
        self.kick_now();
    }

    fn airdrop_send_gone(&mut self, session: u64, key: &str) {
        if session != self.drop.session {
            return;
        }
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        if let Some(bubble) = share.bubbles.iter_mut().find(|bubble| bubble.target.key == key) {
            if let Some(mut child) = bubble.send.take() {
                let _ = child.wait();
            }
            if matches!(bubble.target.state, Status::Waiting | Status::Sending(_)) {
                bubble.target.state = Status::Failed;
                bubble.changed = Instant::now();
                share.status = Some(airdrop::plain_failure("timed out"));
            }
        }
        self.paint_share();
    }

    pub(super) fn airdrop_close_share(&mut self) {
        let Some(mut share) = self.drop.share.take() else {
            return;
        };
        let mut children: Vec<Child> = share.bubbles.iter_mut().filter_map(|bubble| bubble.send.take()).collect();
        children.extend(share.browse.take());
        self.background("airdrop share close", move || {
            for child in children.iter_mut() {
                stop(child);
            }
            |_: &mut State| {}
        });
        self.airdrop_release();
        self.kick_now();
    }

    fn share_look(&self) -> share::Look {
        share::Look::themed(&self.palette, self.look.accent)
    }

    pub(super) fn paint_share(&mut self) {
        let (Some(title), Some(font), Some(small)) = (self.search_font.clone(), self.font.clone(), self.card_font.clone()) else {
            return;
        };
        let look = self.share_look();
        let screen = self.screen;
        let up = self.drop.up;
        let wifi_off = self
            .drop
            .report
            .as_ref()
            .is_some_and(|report| report.mode == Some(airdrop::Mode::Exclusive))
            .then(|| drifter_core::lang::t("Wi-Fi turns off while AirDrop is on, and comes back after."));
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        if share.haze.is_none() {
            self.paint_on_worker("airdrop dim", move || Some(worker::dim(0.22)), |state, art| {
                if let Some(share) = state.drop.share.as_mut() {
                    share.haze = art;
                }
            });
        }
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        if share.check.is_none() {
            let check = look.check();
            if let Ok(Some(tex)) = upload(&mut self.renderer, Some(&check), "airdrop check") {
                share.check = Some(tex);
            }
        }
        let count = share.bubbles.len();
        let empty = match (&share.status, up) {
            (Some(status), _) => status.clone(),
            (None, false) => drifter_core::lang::t("Turning on AirDrop…").to_string(),
            (None, true) => {
                let mut words = drifter_core::lang::t("Looking for nearby devices. On the iPhone, open AirDrop and choose Everyone for 10 minutes.").to_string();
                if let Some(wifi) = wifi_off {
                    words = format!("{words} {wifi}");
                }
                words
            }
        };
        let done = share.bubbles.iter().any(|bubble| bubble.target.state == Status::Sent);
        let key = (count, empty.clone(), done);
        if share.painted.as_ref() != Some(&key) && share.lane.claim() {
            let id = share.lane.id();
            let summary = share::summary(&share.files);
            let rect = look.rect(screen, count);
            share.painted = Some(key);
            let sheet_font = font.clone();
            self.on_worker(
                "airdrop sheet",
                move || look.paint(count, &summary, &empty, done, &title, &sheet_font),
                move |state, image| {
                    let Some(share) = state.drop.share.as_mut().filter(|share| share.lane.id() == id) else {
                        return;
                    };
                    let again = share.lane.release();
                    if let Ok(Some(tex)) = upload(&mut state.renderer, Some(&image), "airdrop sheet") {
                        share.art = Some((tex, rect));
                    }
                    if again {
                        share.painted = None;
                        state.paint_share();
                    }
                    state.kick_now();
                },
            );
        }
        let Some(share) = self.drop.share.as_mut() else {
            return;
        };
        let wanted: Vec<Target> = share
            .bubbles
            .iter()
            .filter(|bubble| bubble.painted.as_ref() != Some(&bubble.target))
            .map(|bubble| bubble.target.clone())
            .collect();
        let rings: Vec<(String, u8)> = share
            .bubbles
            .iter()
            .filter_map(|bubble| match bubble.target.state {
                Status::Sending(pct) if bubble.ring.as_ref().is_none_or(|(_, held)| *held != pct) => Some((bubble.target.key.clone(), pct)),
                _ => None,
            })
            .collect();
        for target in &wanted {
            if let Some(bubble) = share.bubbles.iter_mut().find(|bubble| bubble.target.key == target.key) {
                bubble.painted = Some(target.clone());
            }
        }
        if wanted.is_empty() && rings.is_empty() {
            return;
        }
        self.on_worker(
            "airdrop bubbles",
            move || {
                let arts: Vec<(Target, drifter_core::Image)> = wanted.into_iter().map(|target| {
                    let image = look.paint_target(&target, &font, &small);
                    (target, image)
                }).collect();
                let rings: Vec<(String, u8, drifter_core::Image)> =
                    rings.into_iter().map(|(key, pct)| (key, pct, look.ring(f64::from(pct) / 100.0))).collect();
                (arts, rings)
            },
            |state, (arts, rings)| {
                for (target, image) in arts {
                    let latest = state
                        .drop
                        .share
                        .as_ref()
                        .and_then(|share| share.bubbles.iter().find(|bubble| bubble.target.key == target.key))
                        .is_some_and(|bubble| bubble.painted.as_ref() == Some(&target));
                    if !latest {
                        continue;
                    }
                    let tex = upload(&mut state.renderer, Some(&image), "airdrop bubble").ok().flatten();
                    if let Some(bubble) = state.drop.share.as_mut().and_then(|share| share.bubbles.iter_mut().find(|bubble| bubble.target.key == target.key)) {
                        bubble.art = tex;
                    }
                }
                for (key, pct, image) in rings {
                    let current = state
                        .drop
                        .share
                        .as_ref()
                        .and_then(|share| share.bubbles.iter().find(|bubble| bubble.target.key == key))
                        .is_some_and(|bubble| bubble.target.state == Status::Sending(pct));
                    if !current {
                        continue;
                    }
                    let tex = upload(&mut state.renderer, Some(&image), "airdrop ring").ok().flatten();
                    if let (Some(tex), Some(bubble)) = (tex, state.drop.share.as_mut().and_then(|share| share.bubbles.iter_mut().find(|bubble| bubble.target.key == key))) {
                        bubble.ring = Some((tex, pct));
                    }
                }
                state.kick_now();
            },
        );
    }

    pub(super) fn share_layers(&self) -> Option<(Vec<fade::Layer>, Rect)> {
        let share = self.drop.share.as_ref()?;
        let (art, rect) = share.art.as_ref()?;
        let look = self.share_look();
        let pace = self.window_pace;
        let still = pace <= 0.0 || self.window_still;
        let now = Instant::now();
        let count = share.bubbles.len();
        let mut layers = Vec::new();
        for (index, bubble) in share.bubbles.iter().enumerate() {
            let Some(tex) = bubble.art.as_ref() else {
                continue;
            };
            let cell = look.cell_rect(count, index);
            let mut place = Rect::new(rect.origin + cell.origin, cell.size);
            let (mut alpha, mut scale) = (1.0, 1.0);
            if pace > 0.0 {
                if now < bubble.born {
                    continue;
                }
                let t = now.duration_since(bubble.born).as_secs_f64() / (BUBBLE_IN / pace);
                alpha = drifter_core::motion::smooth(t);
                if !still {
                    scale = BUBBLE_FROM + (1.0 - BUBBLE_FROM) * drifter_core::drawer::ease(t);
                }
            }
            if !still && matches!(bubble.target.state, Status::Declined | Status::Failed) {
                let t = (bubble.changed.elapsed().as_secs_f64() / (SHAKE / pace.max(0.01))).clamp(0.0, 1.0);
                place.origin.x += (t * std::f64::consts::TAU * 3.0).sin() * 9.0 * (1.0 - t) * (1.0 - t);
            }
            let bubble_rect = look.bubble_rect(count, index);
            let centre = rect.origin + bubble_rect.center();
            if bubble.target.state == Status::Sent
                && let Some(check) = share.check.as_ref()
            {
                let t = match still {
                    true => 1.0,
                    false => bubble.changed.elapsed().as_secs_f64() / (POP / pace.max(0.01)),
                };
                let grow = match still {
                    true => 1.0,
                    false => 0.3 + 0.7 * drifter_core::drawer::ease(t),
                };
                let spot = centre + Vec2::new(bubble_rect.size.x * 0.34, bubble_rect.size.y * 0.34);
                layers.push(fade::Layer {
                    alpha: (alpha * drifter_core::motion::smooth(t.min(1.0) * 1.5)) as f32,
                    ..fade::Layer::new(check, Rect::from_center(spot, Vec2::splat(24.0 * grow)), fade::Shadow::None)
                });
            }
            if let (Status::Sending(_) | Status::Waiting, Some((ring, _))) = (&bubble.target.state, bubble.ring.as_ref()) {
                layers.push(fade::Layer {
                    alpha: alpha as f32,
                    ..fade::Layer::new(ring, Rect::from_center(centre, Vec2::splat((look.bubble + 10.0) * scale)), fade::Shadow::None)
                });
            }
            let middle = Vec2::new(place.center().x, place.origin.y + 6.0 + look.bubble / 2.0);
            let drawn = Rect::new(middle + (place.origin - middle) * scale, place.size * scale);
            layers.push(fade::Layer {
                alpha: alpha as f32,
                ..fade::Layer::new(tex, drawn, fade::Shadow::None)
            });
        }
        layers.push(fade::Layer::new(art, *rect, fade::Shadow::Modal));
        if let Some(haze) = share.haze.as_ref() {
            layers.push(fade::Layer {
                scales: false,
                ..fade::Layer::new(haze, Rect::new(Vec2::ZERO, self.screen), fade::Shadow::None)
            });
        }
        Some((layers, *rect))
    }

    pub(super) fn share_look_in(t: f64, still: bool) -> (f64, f64) {
        let scale = match still {
            true => 1.0,
            false => SHEET_FROM + (1.0 - SHEET_FROM) * drifter_core::drawer::ease(t),
        };
        (drifter_core::motion::smooth(t), scale)
    }

    pub(super) fn share_in() -> f64 {
        SHEET_IN
    }

    pub(super) fn share_moving(&self) -> bool {
        let pace = self.window_pace;
        let Some(share) = self.drop.share.as_ref() else {
            return false;
        };
        if pace <= 0.0 {
            return false;
        }
        let now = Instant::now();
        share.bubbles.iter().any(|bubble| {
            now < bubble.born
                || now.duration_since(bubble.born).as_secs_f64() < BUBBLE_IN / pace
                || bubble.changed.elapsed().as_secs_f64() < SHAKE.max(POP) / pace
        })
    }

    pub(super) fn share_press(&mut self) -> bool {
        let Some((rect, count)) = self.drop.share.as_ref().and_then(|share| Some((share.art.as_ref()?.1, share.bubbles.len()))) else {
            return self.drop.share.is_some();
        };
        match self.share_look().hit(count, self.pointer - rect.origin) {
            Some(Press::Cancel) => self.airdrop_close_share(),
            Some(Press::Target(index)) => self.airdrop_send(index),
            None if !rect.contains(self.pointer) => {
                let busy = self
                    .drop
                    .share
                    .as_ref()
                    .is_some_and(|share| share.bubbles.iter().any(|bubble| matches!(bubble.target.state, Status::Waiting | Status::Sending(_))));
                if !busy {
                    self.airdrop_close_share();
                }
            }
            None => {}
        }
        true
    }

    pub(super) fn share_key(&mut self, key: LockKey) {
        if let LockKey::Clear | LockKey::Enter = key {
            self.airdrop_close_share();
        }
    }
}

