use super::*;
use drifter_core::grab::{self, Pick, Slot, Take, Tool};

const CROSS: u32 = 23;
const FLASH: f64 = 0.34;

pub(super) struct Round {
    pub left: Vec<String>,
    pub got: Vec<(Vec2, Vec2, f64, drifter_core::Image)>,
    pub crop: Option<Rect>,
    pub window: Option<u64>,
    pub stash: bool,
    pub began: Instant,
}

pub(super) struct Countdown {
    pub tool: Tool,
    pub area: Option<Rect>,
    pub window: Option<u64>,
    pub clip: bool,
    pub began: Instant,
    pub seconds: u32,
    pub said: u32,
    pub art: Option<(TextureBuffer<Tex>, Vec2, u32, f64)>,
}
const OPT_SHOTS: i32 = 100;
const OPT_DESKTOP: i32 = 101;
const OPT_DOCUMENTS: i32 = 102;
const OPT_CLIPBOARD: i32 = 103;
const OPT_OTHER: i32 = 104;
const OPT_CHOOSE: i32 = 105;
const OPT_TIMER: i32 = 200;
const OPT_THUMBNAIL: i32 = 300;
const OPT_POINTER: i32 = 301;
const OPT_SHADOW: i32 = 302;
const OPT_SOUND: i32 = 303;
const OPT_JOINED: i32 = 304;

pub(super) struct Shooter {
    pub tool: Tool,
    pub bar: Option<(TextureBuffer<Tex>, Rect)>,
    pub hover: Option<Slot>,
    pub from: Option<Vec2>,
    pub chosen: Option<Rect>,
    pub under: Option<Rect>,
    pub tag: Option<(TextureBuffer<Tex>, String, Vec2)>,
    pub tag_want: Option<String>,
    pub sources: Vec<grab::Source>,
    pub bar_lane: worker::Lane,
    pub tag_lane: worker::Lane,
    pub record: bool,
}

pub(super) struct Recording {
    pub child: std::process::Child,
    pub path: PathBuf,
    pub began: Instant,
    pub art: Option<(TextureBuffer<Tex>, Rect)>,
    pub shown: u64,
    pub stopping: bool,
    pub lane: worker::Lane,
}

pub(super) struct Inks {
    pub cross: TextureBuffer<Tex>,
    pub accent: TextureBuffer<Tex>,
    pub sides: Vec<TextureBuffer<Tex>>,
    pub shades: Vec<TextureBuffer<Tex>>,
}

impl State {
    pub(super) fn open_shooter(&mut self, tool: Tool, with_bar: bool) {
        if self.lock.is_some() {
            return;
        }
        self.popover = None;
        self.search = None;
        self.launchpad = None;
        self.control = None;
        self.notes_panel = None;
        self.charges = None;
        self.glance_panel = None;
        self.drop_ghosts();
        self.shooter = Some(Shooter {
            tool,
            bar: None,
            hover: None,
            from: None,
            chosen: None,
            under: None,
            tag: None,
            tag_want: None,
            sources: Vec::new(),
            bar_lane: worker::Lane::default(),
            tag_lane: worker::Lane::default(),
            record: drifter_core::autostart::on_path("wf-recorder"),
        });
        self.shooter_inks();
        if with_bar {
            self.paint_shooter_bar();
        }
        self.aim_shooter();
        self.kick_now();
    }

    pub(super) fn shooter_inks(&mut self) {
        let fresh = self.shot_inks.as_ref().is_some_and(|(accent, _)| *accent == self.look.accent);
        if !fresh {
            let pixel = |rgb: [u8; 3]| drifter_core::Image {
                width: 1,
                height: 1,
                rgba: vec![rgb[0], rgb[1], rgb[2], 255],
            };
            let cross = grab::paint_cross(CROSS);
            let mut four = |rgb: [u8; 3], what: &str| -> Vec<TextureBuffer<Tex>> {
                (0..4)
                    .filter_map(|_| upload(&mut self.renderer, Some(&pixel(rgb)), what).ok().flatten())
                    .collect()
            };
            let sides = four([255, 255, 255], "selection edge");
            let shades = four([0, 0, 0], "selection shade");
            let made = (
                upload(&mut self.renderer, Some(&cross), "crosshair"),
                upload(&mut self.renderer, Some(&pixel(self.look.accent)), "accent"),
            );
            if let (Ok(Some(cross)), Ok(Some(accent)), 4, 4) = (made.0, made.1, sides.len(), shades.len()) {
                self.shot_inks = Some((self.look.accent, Inks {
                    cross,
                    accent,
                    sides,
                    shades,
                }));
            }
        }
    }

    fn bar_look(&self) -> grab::Bar {
        let plain = grab::Bar::themed(&self.palette, self.look.accent);
        grab::Bar {
            back_alpha: self.veil(plain.back_alpha),
            record: self.shooter.as_ref().is_none_or(|shooter| shooter.record),
            ..plain
        }
    }

    pub(super) fn paint_shooter_bar(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let look = self.bar_look();
        let rect = look.rect(self.screen);
        let (tool, hover) = (shooter.tool, shooter.hover);
        let frosting = self.frosting();
        self.paint_in_lane(
            "screenshot bar",
            |state| state.shooter.as_mut().map(|shooter| &mut shooter.bar_lane),
            State::paint_shooter_bar,
            move || {
                let mut image = look.paint(tool, hover, &font);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            move |state, art| {
                if let Some(shooter) = state.shooter.as_mut() {
                    shooter.bar = Some((art, rect));
                }
            },
        );
    }

    pub(super) fn frame_on_screen(&self, index: usize) -> Rect {
        let window = &self.windows[index];
        let chrome = self.chrome;
        let frame = match window.decorated && window.full.is_none() {
            true => Rect::new(
                window.content.origin - Vec2::new(chrome.edge, chrome.title),
                window.content.size + Vec2::new(chrome.edge * 2.0, chrome.title + chrome.edge),
            ),
            false => window.content,
        };
        let shown = self.on_screen(frame);
        grab::drag_rect(shown.origin, shown.max(), self.screen)
    }

    fn window_under_pointer(&self) -> Option<usize> {
        self.windows.iter().enumerate().rev().find_map(|(index, window)| {
            let usable = window.placed && !window.hidden && !window.going;
            (usable && self.frame_on_screen(index).contains(self.pointer)).then_some(index)
        })
    }

    pub(super) fn aim_shooter(&mut self) {
        let Some(tool) = self.shooter.as_ref().map(|shooter| shooter.tool) else {
            return;
        };
        let on_bar = self
            .shooter
            .as_ref()
            .and_then(|shooter| shooter.bar.as_ref())
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        let under = match (tool.pick, on_bar) {
            (Pick::Window, false) => self.window_under_pointer().map(|index| self.frame_on_screen(index)),
            _ => None,
        };
        let hover = match on_bar {
            true => self.shooter.as_ref().and_then(|shooter| {
                let (_, rect) = shooter.bar.as_ref()?;
                self.bar_look().hit(self.pointer - rect.origin)
            }),
            false => None,
        };
        let dragging = self
            .shooter
            .as_ref()
            .and_then(|shooter| shooter.from)
            .map(|from| grab::drag_rect(from, self.pointer, self.screen));
        let record = self.shooter.as_ref().is_none_or(|shooter| shooter.record);
        let words = match (tool.pick, dragging) {
            (_, Some(rect)) => Some(grab::size_tag(rect)),
            (_, None) if on_bar => hover.map(|slot| grab::tip(slot, tool, record)),
            (Pick::Portion, None) => Some(format!("{}  {}", self.pointer.x.round(), self.pointer.y.round())),
            _ => None,
        };
        let Some(shooter) = self.shooter.as_mut() else {
            return;
        };
        shooter.under = under;
        let stale = words.as_ref() != shooter.tag.as_ref().map(|(_, held, _)| held);
        if words.is_none() {
            shooter.tag = None;
        }
        shooter.tag_want = words;
        if stale {
            self.paint_tag();
        }
        let Some(shooter) = self.shooter.as_mut() else {
            return;
        };
        if shooter.hover != hover {
            shooter.hover = hover;
            self.paint_shooter_bar();
        }
        self.kick_now();
    }

    fn paint_tag(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(words) = self.shooter.as_ref().and_then(|shooter| shooter.tag_want.clone()) else {
            return;
        };
        let painted = words.clone();
        self.paint_in_lane_sized(
            "size tag",
            |state| state.shooter.as_mut().map(|shooter| &mut shooter.tag_lane),
            State::paint_tag,
            move || Some(grab::paint_tag(&painted, &font)),
            move |state, art, size| {
                if let Some(shooter) = state.shooter.as_mut()
                    && shooter.tag_want.as_deref() == Some(words.as_str())
                {
                    shooter.tag = Some((art, words, size));
                }
            },
        );
    }

    pub(super) fn shooter_key(&mut self, key: LockKey) {
        match key {
            LockKey::Clear => {
                self.shooter = None;
                self.popover = None;
                self.shot_clip = false;
            }
            LockKey::Letter(' ') => {
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.tool = shooter.tool.spaced();
                    shooter.from = None;
                    shooter.chosen = None;
                }
                if self.shooter.as_ref().is_some_and(|shooter| shooter.bar.is_some()) {
                    self.paint_shooter_bar();
                }
                self.aim_shooter();
            }
            LockKey::Enter => self.shoot(),
            LockKey::Vt(vt) => self.change_vt(vt),
            _ => {}
        }
        self.kick_now();
    }

    pub(super) fn press_shooter(&mut self, code: u32) -> bool {
        let Some(shooter) = self.shooter.as_ref() else {
            return false;
        };
        if code != BTN_LEFT {
            return true;
        }
        if let Some((_, rect)) = shooter.bar.as_ref()
            && rect.contains(self.pointer)
        {
            let rect = *rect;
            match self.bar_look().hit(self.pointer - rect.origin) {
                Some(Slot::Close) => {
                    self.shooter = None;
                    self.shot_clip = false;
                }
                Some(Slot::Tool(tool)) if tool.take == Take::Video && !shooter.record => self.needs_recorder(),
                Some(Slot::Tool(tool)) => {
                    if let Some(shooter) = self.shooter.as_mut() {
                        shooter.tool = tool;
                        if tool.pick != Pick::Portion {
                            shooter.chosen = None;
                        }
                    }
                    self.paint_shooter_bar();
                }
                Some(Slot::Options) => self.shooter_options(rect),
                Some(Slot::Go) if shooter.tool.take == Take::Video && !shooter.record => self.needs_recorder(),
                Some(Slot::Go) => self.shoot(),
                None => {}
            }
            self.kick_now();
            return true;
        }
        match shooter.tool.pick {
            Pick::Screen => self.shoot(),
            Pick::Window => {
                if let Some(index) = self.window_under_pointer() {
                    let index = self.raise(index);
                    let rect = self.frame_on_screen(index);
                    self.shot_window = Some(self.windows[index].id);
                    self.fire(Some(rect));
                }
            }
            Pick::Portion => {
                let at = self.pointer;
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.from = Some(at);
                    shooter.chosen = None;
                }
                self.aim_shooter();
            }
        }
        true
    }

    pub(super) fn release_shooter(&mut self) {
        let Some(from) = self.shooter.as_mut().and_then(|shooter| shooter.from.take()) else {
            return;
        };
        let rect = grab::drag_rect(from, self.pointer, self.screen);
        if !grab::big_enough(rect) {
            self.aim_shooter();
            return;
        }
        let with_bar = self.shooter.as_ref().is_some_and(|shooter| shooter.bar.is_some());
        match with_bar {
            true => {
                if let Some(shooter) = self.shooter.as_mut() {
                    shooter.chosen = Some(rect);
                }
                self.aim_shooter();
            }
            false => self.fire(Some(rect)),
        }
    }

    fn needs_recorder(&mut self) {
        let install = [("apt", "sudo apt install wf-recorder"), ("dnf", "sudo dnf install wf-recorder"), ("pacman", "sudo pacman -S wf-recorder"), ("zypper", "sudo zypper install wf-recorder")]
            .into_iter()
            .find(|(tool, _)| drifter_core::autostart::on_path(tool))
            .map(|(_, line)| line)
            .unwrap_or("wf-recorder");
        self.shot_note(
            drifter_core::lang::t("Screen recording needs wf-recorder"),
            drifter_core::lang::tf("Install it with: {}", &[&install]),
            None,
        );
    }

    fn shoot(&mut self) {
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let area = match shooter.tool.pick {
            Pick::Screen => None,
            Pick::Window => match self.window_under_pointer() {
                Some(index) => {
                    let index = self.raise(index);
                    self.shot_window = Some(self.windows[index].id);
                    Some(self.frame_on_screen(index))
                }
                None => return,
            },
            Pick::Portion => match shooter.chosen {
                Some(rect) => Some(rect),
                None => return,
            },
        };
        self.fire(area);
    }

    fn fire(&mut self, area: Option<Rect>) {
        let Some(shooter) = self.shooter.take() else {
            return;
        };
        self.shot_clip = self.shot_clip || self.wayland.keyboard.modifier_state().ctrl;
        self.popover = None;
        self.shot_tool = shooter.tool;
        if self.shots.timer > 0 {
            self.count_down(shooter.tool, area);
            return;
        }
        match shooter.tool.take {
            Take::Still => self.take_still(area),
            Take::Video => {
                self.shot_window = None;
                self.shot_clip = false;
                self.start_recording(area);
            }
        }
    }

    pub(super) fn take_still(&mut self, area: Option<Rect>) {
        if self.shot_round.is_some() {
            return;
        }
        self.drop_ghosts();
        self.shooter_inks();
        let window = self.shot_window.take();
        let left: Vec<String> = match area.is_some() || window.is_some() {
            true => self.heads.get(self.hand).map(|head| vec![head.name.clone()]).unwrap_or_default(),
            false => self
                .heads
                .iter()
                .filter(|head| self.mirror_source(head).is_none())
                .map(|head| head.name.clone())
                .collect(),
        };
        self.shot_round = Some(Round {
            left,
            got: Vec::new(),
            crop: area,
            window,
            stash: self.shots.clipboard != std::mem::take(&mut self.shot_clip),
            began: Instant::now(),
        });
        self.kick_now();
    }

    fn count_down(&mut self, tool: Tool, area: Option<Rect>) {
        let seconds = self.shots.timer.min(60);
        self.shot_count = Some(Countdown {
            tool,
            area,
            window: self.shot_window.take(),
            clip: std::mem::take(&mut self.shot_clip),
            began: Instant::now(),
            seconds,
            said: 0,
            art: None,
        });
        self.shooter_inks();
        self.wind_the_countdown();
    }

    pub(super) fn wind_the_countdown(&mut self) {
        let Some(count) = self.shot_count.as_ref() else {
            return;
        };
        let total = f64::from(count.seconds);
        let gone = count.began.elapsed().as_secs_f64();
        if gone >= total {
            let Some(count) = self.shot_count.take() else {
                return;
            };
            self.shot_window = count.window;
            self.shot_clip = count.clip;
            match count.tool.take {
                Take::Still => self.take_still(count.area),
                Take::Video => {
                    self.shot_window = None;
                    self.shot_clip = false;
                    self.start_recording(count.area);
                }
            }
            return;
        }
        let left = (total - gone).ceil().max(1.0) as u32;
        let share = (total - gone) / total;
        if count.said != left {
            self.play(drifter_core::shutter::Sound::Tick);
            if let Some(count) = self.shot_count.as_mut() {
                count.said = left;
            }
        }
        let Some(count) = self.shot_count.as_ref() else {
            return;
        };
        let stale = count.art.as_ref().is_none_or(|(_, _, said, drawn)| *said != left || (drawn - share).abs() > 0.004);
        if !stale {
            return;
        }
        let Some(big) = self.lock_big.clone() else {
            return;
        };
        let caption = self.font.as_ref().map(|font| font.paint(drifter_core::lang::t("Esc to cancel"), [255, 255, 255]));
        let image = grab::paint_countdown(&big.paint(&left.to_string(), [255, 255, 255]), caption.as_ref(), share, self.look.accent);
        let size = drifter_core::paint::logical(&image);
        if let Ok(Some(art)) = upload(&mut self.renderer, Some(&image), "countdown")
            && let Some(count) = self.shot_count.as_mut()
        {
            count.art = Some((art, size, left, share));
        }
    }

    pub(super) fn countdown_rect(&self) -> Option<Rect> {
        let count = self.shot_count.as_ref()?;
        let (_, size, _, _) = count.art.as_ref()?;
        let middle = match (count.window.and_then(|id| self.place_of(id)), count.area) {
            (Some(index), _) => self.frame_on_screen(index).center(),
            (None, Some(area)) => area.center(),
            (None, None) => self.screen / 2.0,
        };
        Some(Rect::new((middle - *size / 2.0).round(), *size))
    }

    pub(super) fn cancel_countdown(&mut self) -> bool {
        if self.shot_count.take().is_none() {
            return false;
        }
        self.kick_now();
        true
    }

    pub(super) fn press_countdown(&mut self, code: u32) -> bool {
        if self.countdown_rect().is_some_and(|rect| rect.contains(self.pointer)) {
            if code == BTN_LEFT {
                self.cancel_countdown();
            }
            return true;
        }
        false
    }

    pub(super) fn play(&self, sound: drifter_core::shutter::Sound) {
        if !self.shots.sound {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = drifter_core::grab::stash_folder(&home).with_file_name("sounds");
        self.background("drifter-sound", move || {
            let path = folder.join(sound.file_name());
            let wav = sound.wav();
            let fresh = std::fs::metadata(&path).is_ok_and(|meta| meta.len() == wav.len() as u64);
            if !fresh {
                let part = folder.join(format!(".{}.part", sound.file_name()));
                let _ = std::fs::create_dir_all(&folder)
                    .and_then(|()| std::fs::write(&part, &wav))
                    .and_then(|()| std::fs::rename(&part, &path));
            }
            let file = path.display().to_string();
            for player in [vec!["pw-play", file.as_str()], vec!["paplay", file.as_str()], vec!["aplay", "-q", file.as_str()]] {
                let spawned = std::process::Command::new(player[0])
                    .args(&player[1..])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if let Ok(mut child) = spawned {
                    let _ = child.wait();
                    break;
                }
            }
            |_: &mut State| {}
        });
    }

    pub(super) fn shot_flash_elements(&self, elements: &mut Vec<Skin>) {
        let Some((_, inks)) = self.shot_inks.as_ref() else {
            return;
        };
        let Some(head) = self.heads.get(self.at) else {
            return;
        };
        for (name, rect, since) in &self.shot_flash {
            if *name != head.name {
                continue;
            }
            let t = (since.elapsed().as_secs_f64() / FLASH).min(1.0);
            let alpha = (1.0 - drifter_core::motion::smooth(t)) * 0.7;
            if alpha > 0.0 {
                elements.push(Skin::Texture(fill(&inks.sides[0], *rect, alpha as f32)));
            }
        }
    }

    pub(super) fn wind_the_flash(&mut self) {
        self.shot_flash.retain(|(_, _, since)| since.elapsed().as_secs_f64() < FLASH);
        if self.shot_count.is_some() {
            self.wind_the_countdown();
        }
        if let Some(round) = self.shot_round.as_ref()
            && round.began.elapsed() > Duration::from_millis(1500)
        {
            log!("drifter: a display did not draw in time, so the screenshot leaves it out");
            self.finish_round();
        }
    }

    pub(super) fn shots_moving(&self) -> bool {
        !self.shot_flash.is_empty() || self.shot_count.is_some() || self.shot_round.is_some()
    }

    fn shooter_options(&mut self, bar: Rect) {
        self.background("sound-sources", move || {
            let listing = std::process::Command::new("pactl")
                .args(["list", "sources"])
                .env("LC_ALL", "C")
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
            move |state: &mut State| {
                if state.shooter.is_some() {
                    state.shooter_sources(bar, listing.as_deref());
                }
            }
        });
    }

    fn shot_place(&self) -> i32 {
        if self.shots.clipboard {
            return OPT_CLIPBOARD;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = self.shots.folder_in(&home);
        let named = |key: &str, fallback: &str| drifter_core::folders::user_dir(&home, key, fallback) == folder;
        match self.shots.folder.is_none() {
            true => OPT_SHOTS,
            false if named("DESKTOP", "Desktop") => OPT_DESKTOP,
            false if named("DOCUMENTS", "Documents") => OPT_DOCUMENTS,
            false => OPT_OTHER,
        }
    }

    fn shooter_sources(&mut self, bar: Rect, listing: Option<&str>) {
        use drifter_core::lang::t;
        use drifter_core::menu::Item;
        let video = self.shooter.as_ref().is_some_and(|shooter| shooter.tool.take == Take::Video);
        let mark = |on: bool, label: &str| match on {
            true => format!("•  {label}"),
            false => format!("    {label}"),
        };
        let head = |id: i32, label: &str| Item {
            enabled: false,
            ..Item::line(id, label)
        };
        let mut items = Vec::new();
        let place = self.shot_place();
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let other = match place {
            OPT_OTHER => {
                let folder = self.shots.folder_in(&home);
                let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                Some(name)
            }
            _ => None,
        };
        items.push(head(-10, t("Save to")));
        items.push(Item::line(OPT_SHOTS, &mark(place == OPT_SHOTS, t("Screenshots"))));
        items.push(Item::line(OPT_DESKTOP, &mark(place == OPT_DESKTOP, t("Desktop"))));
        items.push(Item::line(OPT_DOCUMENTS, &mark(place == OPT_DOCUMENTS, t("Documents"))));
        if !video {
            items.push(Item::line(OPT_CLIPBOARD, &mark(place == OPT_CLIPBOARD, t("Clipboard"))));
        }
        if let Some(name) = other.as_deref() {
            items.push(Item::line(OPT_OTHER, &mark(true, name)));
        }
        items.push(Item::line(OPT_CHOOSE, &mark(false, t("Other Folder…"))));
        items.push(Item::rule(-11));
        items.push(head(-12, t("Timer")));
        for (id, label) in [(OPT_TIMER, t("None")), (OPT_TIMER + 5, t("5 Seconds")), (OPT_TIMER + 10, t("10 Seconds"))] {
            items.push(Item::line(id, &mark(self.shots.timer == (id - OPT_TIMER) as u32, label)));
        }
        items.push(Item::rule(-13));
        items.push(head(-14, t("Options")));
        let toggles = [
            (OPT_THUMBNAIL, self.shots.thumbnail, t("Show Floating Thumbnail")),
            (OPT_POINTER, self.shots.pointer, t("Show Mouse Pointer")),
            (OPT_SHADOW, self.shots.shadow, t("Window Shadow")),
            (OPT_SOUND, self.shots.sound, t("Play Sounds")),
        ];
        for (id, on, label) in toggles {
            items.push(Item::line(id, &mark(on, label)));
        }
        if self.heads.iter().filter(|head| head.mirror.is_none()).count() > 1 {
            items.push(Item::line(OPT_JOINED, &mark(self.shots.joined, t("All Displays in One Picture"))));
        }
        let sources = listing.map(grab::sources);
        if video {
            items.push(Item::rule(-15));
            items.push(head(-16, t("Sound")));
            match sources.as_ref() {
                Some(sources) => {
                    items.push(Item::line(0, &mark(self.sound_from.is_none(), t("None"))));
                    for (at, source) in sources.iter().enumerate() {
                        let on = self.sound_from.as_deref() == Some(source.name.as_str());
                        items.push(Item::line(at as i32 + 1, &mark(on, &source.label)));
                    }
                }
                None => items.push(head(-17, t("Sound needs pactl (pulseaudio-utils)"))),
            }
        }
        if let Some(shooter) = self.shooter.as_mut() {
            shooter.sources = sources.unwrap_or_default();
        }
        let look = self.bar_look();
        let options = look
            .slots()
            .into_iter()
            .find(|(_, slot)| *slot == Slot::Options)
            .map(|(rect, _)| rect)
            .unwrap_or(Rect::new(Vec2::ZERO, bar.size));
        let size = self.menu_look().size(&items, self.font.as_deref(), self.card_font.as_deref());
        let top = (bar.origin.y - size.y - 8.0).max(4.0);
        let at = Vec2::new(bar.origin.x + options.origin.x, top);
        let from = Vec2::new(bar.origin.x + options.center().x, bar.origin.y);
        self.open_popover_from(items, at, from, Target::Shooter);
    }

    pub(super) fn shooter_menu_pick(&mut self, id: i32) {
        self.popover = None;
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let before = self.shots.clone();
        match id {
            0..OPT_SHOTS => {
                self.sound_from = match id {
                    0 => None,
                    id => self
                        .shooter
                        .as_ref()
                        .and_then(|shooter| shooter.sources.get(id as usize - 1))
                        .map(|source| source.name.clone()),
                };
            }
            OPT_SHOTS => {
                self.shots.folder = None;
                self.shots.clipboard = false;
            }
            OPT_DESKTOP | OPT_DOCUMENTS => {
                let (key, fallback) = match id {
                    OPT_DESKTOP => ("DESKTOP", "Desktop"),
                    _ => ("DOCUMENTS", "Documents"),
                };
                let folder = drifter_core::folders::user_dir(&home, key, fallback);
                let shown = match folder.strip_prefix(&home) {
                    Ok(rest) => format!("~/{}", rest.display()),
                    Err(_) => folder.display().to_string(),
                };
                self.shots.folder = Some(shown);
                self.shots.clipboard = false;
            }
            OPT_CLIPBOARD => self.shots.clipboard = true,
            OPT_OTHER => self.shots.clipboard = false,
            OPT_CHOOSE => {
                self.shooter = None;
                self.shot_clip = false;
                self.open_settings_at("shortcuts");
            }
            OPT_JOINED => self.shots.joined = !self.shots.joined,
            OPT_THUMBNAIL => self.shots.thumbnail = !self.shots.thumbnail,
            OPT_POINTER => self.shots.pointer = !self.shots.pointer,
            OPT_SHADOW => self.shots.shadow = !self.shots.shadow,
            OPT_SOUND => self.shots.sound = !self.shots.sound,
            id if (OPT_TIMER..=OPT_TIMER + 10).contains(&id) => self.shots.timer = (id - OPT_TIMER) as u32,
            _ => {}
        }
        if self.shots != before {
            self.store_shots();
        }
        self.kick_now();
    }

    fn store_shots(&mut self) {
        let path = self.config_path.clone();
        let written = std::fs::read_to_string(&path)
            .or_else(|error| match error.kind() {
                std::io::ErrorKind::NotFound => Ok(String::new()),
                _ => Err(error),
            })
            .map_err(|error| format!("cannot read {}: {error}", path.display()))
            .and_then(|text| self.shots.store(&text))
            .and_then(|text| {
                let fresh = path.with_extension("toml.new");
                std::fs::write(&fresh, text)
                    .and_then(|()| std::fs::rename(&fresh, &path))
                    .map_err(|error| format!("cannot write {}: {error}", path.display()))
            });
        if let Err(error) = written {
            log!("drifter: the screenshot options are not saved: {error}");
        }
    }

    fn start_recording(&mut self, area: Option<Rect>) {
        if self.recording.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = self.shots.folder_in(&home);
        let stamp = drifter_core::tray::now("%Y-%m-%d at %H.%M.%S").unwrap_or_default();
        let path = folder.join(grab::file_name(Take::Video, &stamp, "mp4"));
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.recording_done(Err(error.to_string()));
            return;
        }
        let Some(head) = self.heads.get(self.hand) else {
            return;
        };
        let area = area.map(|rect| Rect::new(rect.origin + head.place, rect.size));
        let args = grab::recorder_args(&head.name, area, self.sound_from.as_deref(), &path);
        let spawned = crate::apps::fresh_signals(&mut std::process::Command::new("wf-recorder"))
            .args(&args)
            .env("WAYLAND_DISPLAY", &self.socket)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let child = match spawned {
            Ok(child) => child,
            Err(error) => {
                self.needs_recorder();
                log!("drifter: wf-recorder did not start: {error}");
                return;
            }
        };
        self.play(drifter_core::shutter::Sound::Start);
        self.recording = Some(Recording {
            child,
            path,
            began: Instant::now(),
            art: None,
            shown: u64::MAX,
            stopping: false,
            lane: worker::Lane::default(),
        });
        self.paint_stop();
        let _ = self.handle.insert_source(
            Timer::from_duration(Duration::from_millis(250)),
            |_, _, state: &mut State| match state.tick_recording() {
                true => TimeoutAction::ToDuration(Duration::from_millis(250)),
                false => TimeoutAction::Drop,
            },
        );
        self.kick_now();
    }

    fn paint_stop(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let Some(recording) = self.recording.as_ref() else {
            return;
        };
        let seconds = recording.began.elapsed().as_secs();
        if seconds == recording.shown {
            return;
        }
        let screen = self.screen;
        self.paint_in_lane_sized(
            "recording",
            |state| state.recording.as_mut().map(|recording| &mut recording.lane),
            State::paint_stop,
            move || Some(grab::paint_stop(seconds, &font)),
            move |state, art, size| {
                let rect = Rect::new(Vec2::new(((screen.x - size.x) / 2.0).round(), 4.0), size);
                if let Some(recording) = state.recording.as_mut() {
                    recording.art = Some((art, rect));
                    recording.shown = seconds;
                }
            },
        );
    }

    fn tick_recording(&mut self) -> bool {
        let Some(recording) = self.recording.as_mut() else {
            return false;
        };
        match recording.child.try_wait() {
            Ok(None) => {
                if recording.stopping {
                    self.unblock_copies();
                } else {
                    self.paint_stop();
                }
                true
            }
            Ok(Some(_)) | Err(_) => {
                let path = recording.path.clone();
                self.recording = None;
                let saved = std::fs::metadata(&path)
                    .ok()
                    .filter(|meta| meta.len() > 0)
                    .map(|_| path)
                    .ok_or_else(|| "the recorder stopped before it saved anything".to_string());
                self.recording_done(saved);
                self.kick_now();
                false
            }
        }
    }

    fn unblock_copies(&mut self) {
        for copy in &mut self.copies {
            copy.damage = false;
        }
        self.kick_now();
    }

    pub(super) fn stop_recording(&mut self) {
        let Some(recording) = self.recording.as_mut() else {
            return;
        };
        if !recording.stopping {
            recording.stopping = true;
            unsafe { libc::kill(recording.child.id() as i32, libc::SIGINT) };
        }
        recording.art = None;
        self.unblock_copies();
    }

    fn recording_done(&mut self, saved: Result<PathBuf, String>) {
        if saved.is_ok() {
            self.play(drifter_core::shutter::Sound::Stop);
        }
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let (summary, body) = match saved {
            Ok(path) => {
                if let Some(folder) = path.parent() {
                    self.note_folders.insert(id, folder.to_path_buf());
                }
                (drifter_core::lang::t("Screen recording saved").to_string(), path.display().to_string())
            }
            Err(error) => (drifter_core::lang::t("No screen recording").to_string(), error),
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Drifter").to_string(),
            summary,
            body,
            actions: Vec::new(),
            expire: Some(drifter_core::notify::DEFAULT_MS),
            asked: -1,
            critical: false,
            icon: None,
        });
    }

    pub(super) fn press_stop(&mut self, code: u32) -> bool {
        let on = self
            .recording
            .as_ref()
            .and_then(|recording| recording.art.as_ref())
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        if on && code == BTN_LEFT {
            self.stop_recording();
        }
        on
    }

    pub(super) fn own_elements(&self, elements: &mut Vec<Skin>) {
        if let (Some(rect), Some((art, ..))) = (self.countdown_rect(), self.shot_count.as_ref().and_then(|count| count.art.as_ref())) {
            elements.push(Skin::Texture(element(art, rect, 1.0)));
        }
        if self.shooter.is_none() && self.shot_menu_open() {
            self.popover_elements(elements);
        }
        if let Some((art, rect)) = self.recording.as_ref().and_then(|recording| recording.art.as_ref()) {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }
        self.thumb_elements(elements);
    }

    pub(super) fn shooter_cursor(&self) -> Option<Vec<Skin>> {
        let shooter = self.shooter.as_ref()?;
        let on_bar = shooter
            .bar
            .as_ref()
            .is_some_and(|(_, rect)| rect.contains(self.pointer));
        if on_bar || shooter.tool.pick != Pick::Portion {
            return None;
        }
        let (_, inks) = self.shot_inks.as_ref()?;
        let mut elements = Vec::new();
        if let Some((art, _, size)) = shooter.tag.as_ref() {
            let size = *size;
            let at = self.pointer + Vec2::new(14.0, 14.0);
            let at = at.min(self.screen - size - Vec2::splat(4.0));
            elements.push(Skin::Texture(element(art, Rect::new(at, size), 1.0)));
        }
        let size = Vec2::splat(CROSS as f64);
        let rect = Rect::new(self.pointer - (size / 2.0).round(), size);
        elements.push(Skin::Texture(element(&inks.cross, rect, 1.0)));
        Some(elements)
    }

    pub(super) fn shooter_elements(&self, elements: &mut Vec<Skin>) {
        let Some(shooter) = self.shooter.as_ref() else {
            return;
        };
        let Some((_, inks)) = self.shot_inks.as_ref() else {
            return;
        };
        self.popover_elements(elements);
        if let (Some((_, bar)), Some(slot), Some((art, _, size))) = (shooter.bar.as_ref(), shooter.hover, shooter.tag.as_ref())
            && bar.contains(self.pointer)
            && let Some((spot, _)) = self.bar_look().slots().into_iter().find(|(_, each)| *each == slot)
        {
            let x = (bar.origin.x + spot.center().x - size.x / 2.0).clamp(4.0, self.screen.x - size.x - 4.0);
            let at = Vec2::new(x, bar.origin.y - size.y - 8.0).round();
            elements.push(Skin::Texture(element(art, Rect::new(at, *size), 1.0)));
        }
        if let Some((art, rect)) = shooter.bar.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
            self.modal_shadow(*rect, elements);
        }
        let boxed = shooter
            .from
            .map(|from| grab::drag_rect(from, self.pointer, self.screen))
            .or(shooter.chosen);
        if let Some(rect) = boxed {
            let thick = 1.0;
            let (o, s) = (rect.origin, rect.size);
            for (side, ink) in [
                Rect::new(o, Vec2::new(s.x, thick)),
                Rect::new(Vec2::new(o.x, o.y + s.y - thick), Vec2::new(s.x, thick)),
                Rect::new(o, Vec2::new(thick, s.y)),
                Rect::new(Vec2::new(o.x + s.x - thick, o.y), Vec2::new(thick, s.y)),
            ]
            .into_iter()
            .zip(&inks.sides)
            {
                elements.push(Skin::Texture(fill(ink, side, 0.9)));
            }
            let screen = self.screen;
            let max = rect.max();
            for (shade, ink) in [
                Rect::new(Vec2::ZERO, Vec2::new(screen.x, o.y)),
                Rect::new(Vec2::new(0.0, max.y), Vec2::new(screen.x, screen.y - max.y)),
                Rect::new(Vec2::new(0.0, o.y), Vec2::new(o.x, s.y)),
                Rect::new(Vec2::new(max.x, o.y), Vec2::new(screen.x - max.x, s.y)),
            ]
            .into_iter()
            .zip(&inks.shades)
            {
                if shade.size.x > 0.0 && shade.size.y > 0.0 {
                    elements.push(Skin::Texture(fill(ink, shade, 0.35)));
                }
            }
        }
        if let Some(rect) = shooter.under {
            elements.push(Skin::Texture(fill(&inks.accent, rect, 0.3)));
        }
    }
}
