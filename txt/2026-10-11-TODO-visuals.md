# Visuals punch list: motion, panels, chrome, shaders

Source: `txt/visuals.md` (25 items + the Animation settings plan). Every claim below was checked against the code at HEAD `d0be046` (branch `icons`). Each item names the file:line to change, the fix, and the test to add. Line numbers drift: re-check them before each edit.

Work rules:
- Numbers are the work order: G1, G2, G3, then 1 to 25. Item numbers match `txt/visuals.md`. Each item says what it waits on. One commit per item.
- `render.rs` is 1862 lines. Do not grow it. Add only State fields and one-line calls there. New code goes in the new files each item names.
- Every new `lang::t("...")` string must get a line in `crates/drifter-core/lang/{es,de,fr}.txt`, or the test `every_string_in_the_code_has_a_translation` (lang.rs:157) fails. Strings that need entries: Animation, Fade, Reveal, Lift, Magnify, Top, Bottom, Genie, Slight, More, None, Glow, Ripple, Reacts to pointer, Smooth scrolling, Countdown. (Off, On, Still, Calm, Full, Scale, Size already exist.)
- Segments are at most 84 px each (settings.rs:2689, 2846). Keep option labels to one short word.

How motion works today (all items use this):
- Frames: `animating()` (render.rs:683-701) makes `waiting()` return `TICK` 8 ms (render.rs:743-749). A new animation adds one `|| self.x_busy()` term there. Pattern: `desk_menu_busy()` (deskmenu.rs:364-373).
- Easing: `drifter_core::motion::smooth` (motion.rs:80, cubic out, no bounce). `drifter_core::drawer::ease` (drawer.rs:81, back-out, overshoot 1.2). `motion::grow(rect, f)` (motion.rs:85) scales about the centre. `motion::between(a, b, t)` lerps rects.
- Pace: `State.window_pace` (render.rs:546), from `window_pace()` (render.rs:1638). Read only by `step_windows` (windows.rs:30-33) and the desk menu (deskmenu.rs:329).
- GPU draw helpers: `element(buf, rect, alpha)` (render.rs:1801, stretches the whole texture), `faded(buf, src, dst, alpha)` (render.rs:1842, crops), `cropped` (render.rs:1838), `fill(pixel, rect, alpha)` (render.rs:1815). `self.plain` is a 1x1 white pixel (start.rs:362-367); `fill` cannot tint.
- Elements are front to back: push first = on top. So push a panel's art before its shadow.
- `Skin` has only Texture, Client, Clipped (render.rs:128-133). A GPU shader effect renders into an offscreen `wallpaper::Sheet` (wallpaper.rs:158-208) and is shown as a texture, as `paint_logo` does (wallpaper.rs:244-285). Shaders are GLES only; Pixman (the test renderer) must fall back.
- `elements()` runs once per head (render.rs:1291). Per-frame state changes go in `draw()`, not in `elements()`.

---

## Phase 0: Groundwork

- [x] **G1 [BUG] `waiting()` starves every animation while the HUD, a thumbnail or a notification is up.**
  - Waits on: nothing.
  - render.rs:734-742 returns early (HUD: `until - now` up to 1.5 s; thumb: 200 ms; cards: `SECOND_TICK` 250 ms) before `animating()` is checked at :745.
  - Effect: with a note card on screen, every animation falls to 250 ms steps once a frame has no damage.
  - Fix: insert `if self.animating() { return Some(TICK); }` after the lock block, before render.rs:734.
  - Test (render/tests.rs): show a card, start a window open, assert `state.waiting() == Some(TICK)`.

- [x] **G2 [HIGH] One Animation setting; split "speed" from "reduce motion".**
  - Waits on: G1.
  - Today: `window_pace` (render.rs:1638-1645) comes from `Motion::of(reduce, wallpaper_motion)` (welcome.rs:131-137). The wallpaper slider sets window speed, and reduce motion forces pace 0, which kills fades too.
  - Also wrong today: bars and camera read only reduce (`Tunables.still()`, start.rs:186-189, follow.rs:605-609, config.rs:1246-1252). Wallpaper drift ignores reduce (render.rs:306, follow.rs:682).
  - There is no Motion page in Settings (sections at settings.rs:62-86). The `[motion]` table already exists (`Motion`, config.rs:706-774) and feeds the Desktop page (`motion_groups`, pages.rs:76-157). Put the new key and group there.
  - Steps:
    1. config.rs ~705: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)] #[serde(rename_all = "lowercase")] pub enum Animation { Off, Calm, #[default] Full }` with `ALL`, `pace()` (0.0 / 0.75 / 1.0) and `word()`. Add `#[serde(default)] pub animation: Animation` to `Motion` (config.rs:708-729) and its Default (759-773). Re-export in lib.rs:70-74. Document in conf/default.toml `[motion]` (:55-73).
    2. render.rs:1638: `fn window_pace(config: &Config) -> f64 { config.motion.animation.pace() }`. Call sites stay: start.rs:522, follow.rs:580.
    3. Add `State.window_still: bool` next to render.rs:546. Set it to `config.access.reduce_motion` beside start.rs:522 and follow.rs:580.
    4. motion.rs next to `shape` (:59-77): `pub fn shape_for(&self, window: Rect, still: bool) -> (Rect, f32)`. When `still`: Opening and Restoring `(window, t)`, Closing and Minimizing `(window, 1 - t)`, Resizing `(window, 1.0)`. Call it at render.rs:998 in place of `shape`.
    5. deskmenu.rs:329-348: split `still` into `instant = window_pace <= 0.0` and `fade_only = window_still`. Fade only keeps the alpha `(t * 1.6).min(1)` and draws at `*rect` (no scale, no 6 px slide).
    6. config.rs ~1246: `Tunables::paced(self, pace, reduce)`: `still()` when pace is 0 or reduce, else divide `drawer_anim` and `camera_anim` by pace. Use it at start.rs:186-189 and follow.rs:606-609.
    7. Wallpaper drift: in `Paper::from` (render.rs:306) and follow.rs:682, use 0 when reduce is on.
    8. follow.rs:496: change `config.access.reduce_motion || pixman` to `config.motion.animation == Animation::Off || pixman`. Reduce motion keeps the fade.
    9. follow.rs:753-754 (`tell_access`): also send `enable-animations=false` when animation is Off. Pass the animation value in.
    10. Settings, pages.rs:78: new first group in `motion_groups`: header `t("Animation")`, row `Id::Animation` `Control::Segments { options: vec![t("Off"), t("Calm"), t("Full")], chosen }`, footer `t("Reduce motion in Accessibility keeps fades and stops movement.")`. Set arm near pages.rs:1081: `(Id::Animation, Value::Choice(n)) => self.motion.animation = Animation::ALL[n.min(2)]`. Write after pages.rs:1342: `put(doc, "motion", "animation", word(..))` when changed. Add `Id::Animation` to `enum Id` (settings.rs:2517-2653). Add "animation" to `Section::Desktop.words()` (settings.rs:135).
    11. Welcome card: add `welcome::Motion::animation(self) -> Animation` (Still to Off) near welcome.rs:139. At render/welcome.rs:916-919 add a third write: `write_word(&text, "motion", "animation", ..)`.
  - Every item below uses: `pace = self.window_pace` (0 means instant, no ghosts), `reduce = self.window_still` (keep alpha, drop movement, scale and bounce).
  - Behaviour change: a user with wallpaper motion 0..60 now gets Full window speed unless they set Animation.
  - Tests: extend the round trip at pages.rs:1480-1495 with `Id::Animation`. motion.rs: `reduce_motion_keeps_the_fade_but_not_the_move` (shape_for gives `window` rect and alpha t).

- [x] **G3 [MED] Shared GPU pieces: a rounded fill and sized shadows.**
  - Waits on: nothing.
  - There is no GPU rounded rect. `shadow_slices` (window.rs:603-628) skips the centre cell (:615).
  - Add to drifter-core window.rs beside `shadow_slices`: `pub fn round_slices(rect: Rect, radius: f64) -> Vec<(Rect, Rect)>` (3x3 with centre, no drop, no blur growth). Add to paint.rs: `pub fn round_patch(radius: f64, color: [u8; 3]) -> Image` (side `2 * ceil(r) + 1`, `Canvas::round_rect`, paint.rs:84).
  - New file `crates/drifter/src/render/fade.rs` (also used by item 1). In it:
    - `fn round_fill(&self, patch: &TextureBuffer<Tex>, radius, rect, alpha, elements)` pushing `faded` per slice.
    - `pub(super) enum Shadow { None, Tip, Menu, Modal }` and `fn panel_shadow(&self, rect, kind, alpha, elements)`: Tip `(zoom 0.35, drop 3, 0.22)`, Menu `(0.6, 6, 0.30)`, Modal `(1.0, 8, 0.38)`, all through `shadow_slices` and `window_shadow`.
  - State: `round: Option<TextureBuffer<Tex>>` baked in `palette.shade` (theme.rs:45, :68), uploaded next to `plain` (start.rs:367), rebuilt in `recolor` (follow.rs:345). `accent_round` the same, in `self.look.accent`, rebuilt when the accent changes (follow.rs:283-286).
  - windows.rs:385-392: `modal_shadow` becomes a wrapper for `panel_shadow(rect, Modal, 1.0, ..)`. deskmenu.rs:356-359 calls `panel_shadow(drop, Menu, shown, ..)`.
  - Test (drifter-core window.rs): `round_slices_tile_the_rect_with_no_gap` (mirror `the_window_shadow_fades_out_and_has_no_gap`).

---

## Tier 1: small changes, big effect

- [x] **1 [HIGH] Fade every panel in and out from one place.**
  - Waits on: G1, G2, G3.
  - Today 14 panels draw at alpha 1.0 and drop in one frame. Draw sites: render.rs:815-826 (prompt + haze, HUD, cards), 867-870 (search), 875-910 (popover, launchpad, control, notes, charges, glance, glance card, tip), `switch_elements` overlays.rs:461-477, `alert_elements` deskmenu.rs:676-683.
  - State fields: popover :576, tip :599, search :533, launchpad :534, control :524, notes_panel :528, charges :529, glance_panel :530, glance_card :516, hud :655, cards :654, switch :581, alert :579, prompt_art :637, prompt_haze :638. Also desk_menu :577: it grows in already but vanishes on close.
  - There are 109 close sites, not 71. They stay unchanged: the fader compares this frame to the last one.
  - Panel art is `TextureBuffer<Tex>`. It clones cheaply (deskmenu.rs:291-301). Art is `None` only before the first paint, so "visible" means "art present".
  - Fix, in `render/fade.rs`:
    - `enum Pane { Menu, DeskMenu, Tip, Search, Launchpad, Control, Notes, Charges, Glance, GlanceCard, Hud, Card(u32), Switch, Alert, Prompt }`.
    - `struct Layer { art: TextureBuffer<Tex>, rect: Rect, scales: bool }` (`scales = false` for the prompt haze).
    - `struct Fader { born: Option<Instant>, last: Vec<Layer>, frame: Rect, shadow: Shadow, gone: Option<Instant> }`. `struct Fades { panes: HashMap<Pane, Fader> }`.
    - Consts: `FADE_IN 0.16`, `FADE_OUT 0.12`, `FROM_SCALE 0.96`.
    - `fn pane_layers(&self, pane) -> Option<(Vec<Layer>, Rect, Shadow)>`. Switch: icons + art, frame `layout.panel`. Prompt: art (scales) + haze. Desk menu: all `arts`. Cards: one `Pane::Card(note.id)` each, from `card_rects()` (overlays.rs:397).
    - `fn watch_panes(&mut self, now)`, called once in `draw()` after the HUD expiry (render.rs:1229-1231). Present and not born: `born = now`. Present: refresh `last`, `frame`, `gone = None`. Absent and born: `gone = Some(now)`, keep `last`. Drop a fader when `gone` is older than `FADE_OUT / pace`.
    - `fn pane_look(&self, f) -> (f32, f64)`: in `a = smooth(born / (FADE_IN / pace))`, out `a = 1 - smooth(gone / (FADE_OUT / pace))`. Scale `FROM_SCALE + 0.04 * a`, or 1 when reduce. Pace 0: `(1, 1)` and no ghosts.
    - `fn pane_elements(&self, pane, elements)`: draw live layers or the `last` ghost, each scaled about the frame centre with `faded(art, Rect(0, art size), drawn, a)`, then `panel_shadow(frame, kind, a)`.
    - Replace each draw site listed above with `self.pane_elements(Pane::X, elements)`. Keep the order and the `live` and `shooter.is_none()` filters. Leave the over-lock draws at render.rs:786-791 as they are.
    - Desk menu: fade-out only. It keeps its own grow in `desk_menu_elements`.
    - Menus (item 2) grow on their own; they take only the fade-out here.
  - State: `fades: fade::Fades` next to render.rs:654, init at start.rs:588. Add `fades_busy()` to `animating()`.
  - Screenshot catch: `take_still` (shoot.rs:381-385) captures the same frame, so a panel closed just before a shot shows as a ghost. Call `self.fades.drop_ghosts()` in `open_shooter` (shoot.rs:60-66) and `take_still`.
  - Tests: motion.rs `pub fn appear(t, reduce) -> (f64, f64)` with `appear_starts_clear_and_small_and_ends_solid_and_whole`. render/tests.rs `a_closed_panel_fades_out_then_leaves_no_trace`, modelled on tests.rs:662-746: show the HUD, set `hud = None`, `kick_now()`, shot still differs; pump 200 ms, shot equals closed.

- [x] **2 [HIGH] All menus grow from the click point with the desk menu's shadow.**
  - Waits on: G2.
  - Desk menu grow: deskmenu.rs:332-348 (level 0 scales 0.35 to 1 about `anchor`, alpha `(t * 1.6).min(1)`, `GROW_WAIT 0.02`, `GROW 0.2` at :7-8). Shadow: :356-359.
  - Popover (render.rs:326-334) has no anchor and no time. Opened by `open_popover` (bars.rs:44-61). Drawn flat at render.rs:875-879, 787-791 (over lock) and shoot.rs:818-822.
  - There is no window title-bar menu. `BTN_RIGHT` is handled only at input.rs:696, launchpad.rs:153, shoot.rs:736. "Title bar" here means the top-bar app-name menu (appname.rs:81, 91, 101).
  - Menus and their anchor: dock app / trash / bar (dock.rs:385, 397, 412, pointer); tray (start.rs:690-700, pointer via bars.rs:307); tray power (bars.rs:387); app name (appname.rs:81-101); launchpad app (launchpad.rs:167); screenshot options (shoot.rs:428-431, top-left of menu, not the click); screenshot thumb (shoot.rs:749); minimized window (bars.rs:347); power key (power.rs:109, follow.rs:191, screen centre); lock power (lock.rs:747).
  - Fix:
    1. deskmenu.rs: make `GROW_WAIT` and `GROW` `pub(super)`. Move :334-343 into `pub(super) fn grown(rect, anchor, gone, instant, reduce) -> (Rect, f64)` (clamp anchor into rect; reduce gives rect + alpha ramp). Move :352-360 into `fn menu_skin(&self, art, rect, drop, shown, elements)`. `desk_menu_elements` calls both.
    2. `Popover` gets `anchor: Vec2` and `opened: Option<Instant>`.
    3. bars.rs: `open_popover_from(items, at, from, target)`; `open_popover` passes `from = at`. Keep the old `opened` when the new target is the same kind (`mem::discriminant`), so tray and Dock Size submenus do not grow again. shoot.rs:431 passes `from = (bar.x + options.center().x, bar.y)`.
    4. Set `opened = Some(now)` when the art first lands (bars.rs:96-99). Otherwise a slow worker paint skips the grow. Same fix at deskmenu.rs:120.
    5. New `fn popover_elements(&self, elements)` in bars.rs. Replace render.rs:787-791, 875-879 and shoot.rs:818-822 with it.
    6. `popover_busy()` in `animating()`.
  - Test: unit test for `grown()` (t 0 = tiny rect at anchor, t 1 = full rect, instant = full rect). Move `grown` to core menu.rs to test it there (tests at menu.rs:361-577).

- [x] **3 [MED] Add the missing shadows.**
  - Waits on: G3 (and 1 for the fade).
  - No shadow today: popover (render.rs:875-879), tip (:908-910), control (:886-890), notes (:891-893), charges (:894-898), glance panel (:899-903), HUD (:821-823), and also cards (:824-826), switcher (overlays.rs:461-477), password prompt (:815-820).
  - Have one: search (:869), launchpad (:884), glance card (:906), alert (deskmenu.rs:681), screenshot bar (shoot.rs:825), desk menu (deskmenu.rs:356-359).
  - Fix: set `Shadow` per pane in `pane_layers` (item 1). Tip: tip. Menu: popover, desk menu, control, notes, charges, glance panel, HUD, cards. Modal: search, launchpad, glance card, alert, prompt art (not the haze), switcher.
  - Ship order: if item 1 is not done yet, call `panel_shadow` directly after each art push.
  - Test: render/tests.rs `an_open_menu_casts_a_shadow` (clone tests.rs:496-517): 4 px below `rect.max().y` is not background, 60 px below is.

- [x] **4 [BUG] Long window titles are squashed, not cut.**
  - Waits on: nothing.
  - windows.rs:345-354: `width = size.x.min(room)` and `element()` stretches the whole title texture into it (`None` source, render.rs:1801-1813).
  - Fix:
    1. drifter-core window.rs after `buttons_width` (:202): `pub const TITLE_FADE: f64 = 24.0; pub const TITLE_FADE_STEPS: usize = 8;` and `pub fn title_cut(art: Vec2, room: f64) -> Vec<(Rect, f64, f32)>` (source rect, x offset, alpha). Fits: one slice at alpha 1. Too long: one solid slice `room - TITLE_FADE` wide, then 8 slices of 3 px with alpha `1 - smooth((i + 0.5) / 8)`.
    2. windows.rs:345-355: for each slice push `faded(buffer, src, show(Rect(at + (off, 0), (src.w, size.y))), dim * a)`. Keep the centring.
  - Not a gradient overlay (skin PNGs break a flat colour). Not baked into the title art (cut point moves on every resize).
  - Test (window.rs tests, :630+): `a_long_title_is_cut_not_squashed_and_fades_at_its_end`: source widths sum to `room`, no source past `art.x`, source width equals draw width, alphas fall to below 0.2, a short title gives one slice.

- [x] **5 [MED] Notifications slide in from the right; cards below glide; position setting.**
  - Waits on: G1, G2.
  - `Card` (render.rs:312-317) has no motion. Drawn at render.rs:824-826 from `card_rects()` (overlays.rs:397-400) via `Look::stack` (notify.rs:290-303, top right only, newest on top). In: `place_note` (overlays.rs:350-380). Out: `close_note` (:382-389).
  - Extra bug: frost is baked at the predicted spot (overlays.rs:333, 342). After a restack the frost stays wrong.
  - Fix:
    1. `Card` gets `born: Instant`, `from_y: f64`, `moved: Instant`, `frosted: f64`. Consts in overlays.rs: `CARD_IN 0.28`, `CARD_GLIDE 0.22`.
    2. `fn card_shown(&self) -> Vec<(Rect, f32)>` beside `card_rects`: `dx = (1 - smooth(born / CARD_IN)) * (width + margin)`, `alpha = (t * 1.6).min(1)`, `y = lerp(from_y, target y, smooth(moved / CARD_GLIDE))`. Reduce: `dx = 0`, no glide, keep alpha. render.rs:824-826 uses it.
    3. `fn restack_cards(&mut self, change)`: snapshot each shown y by id, run `change`, set `from_y` and `moved` on every card whose target moved. Wrap overlays.rs:366-378 and :384. A card replaced by id keeps its `born`.
    4. Refrost: in `expire_cards`, a card at rest with `frosted != rect.y` gets `paint_note(card.note.clone(), false)`.
    5. `cards_moving()` in `animating()`. Needs G1.
    6. Hit tests (`press_card`, `expire_cards`) keep using target rects.
  - Setting "Position":
    - config.rs beside `Notify` (:908-931): `#[serde(rename_all = "kebab-case")] pub enum NotePosition { #[default] TopRight, BottomRight }` (derive Copy, Eq). Field `#[serde(default)] pub position`.
    - notify.rs:290: `stack(&self, screen, heights, position, floor: f64)`. Bottom: start at `screen.y - margin - floor`, stack up, newest nearest the corner. `floor` is the dock height when `!tunables.bar_hides[Bottom]` (not `dock_band()`, it toggles). Callers: overlays.rs:333, :399, notify.rs:458.
    - Settings: `Id::NotePosition` after `NoteSeconds` (settings.rs:2631). Row after pages.rs:403 (the row is labelled "Show each note for"): `Segments [t("Top"), t("Bottom")]` with detail "At the right edge". Set near pages.rs:1133, write near pages.rs:1432.
    - follow.rs:630: when `position` changes, call `repaint_notes()` (overlays.rs:304).
  - Tests: notify.rs `cards_stack_up_from_the_bottom_right` (beside :455-462). pages.rs:1520-1536 round trip with `NotePosition`.

- [x] **6 [MED] HUD fades out and its bar eases.**
  - Waits on: 1.
  - Shown by `show_hud` (overlays.rs:119-137), `HUD_TIME` 1500 ms (render.rs:116), dropped at render.rs:1229-1231. Bar painted on the CPU (hud.rs:80-89, `bar()` private at :71-76, 130x6 at (120, 25), radius 3). Every reading repaints, so the bar jumps.
  - Fix:
    - Fade-out comes from item 1 (`Pane::Hud`).
    - hud.rs: make `bar()` pub. Split `paint` into `paint_frame` (no fill) + fill; keep `paint` for the test at :168-193. Add `pub fn pill(&self) -> Image` (bar size, `self.fill`, radius 3).
    - render.rs:655: `hud: Option<Hud>`, `struct Hud { art, rect, until, from: f64, to: f64, since: Instant }`. `hud_pill: Option<TextureBuffer<Tex>>`, uploaded by `plain` (start.rs:367), again on theme change (follow.rs ~390-401). `const HUD_EASE: f64 = 0.18`.
    - overlays.rs:133-135: `from = shown level now`, `to = muted ? 0 : level`, `since = now`. Paint with `paint_frame`.
    - Draw before the art: body `faded(pill, (0, 0, w - 3, 6), ..)` + right cap `faded(pill, (bar.w - 3, 0, 3, 6), ..)`, `w = bar.w * shown`; skip when `w < 0.5`. Shown = `lerp(from, to, smooth(since / (HUD_EASE / pace)))`; snap at pace 0 or reduce.
    - Include the fill as a ghost layer so the fade-out shows it.
    - `hud_busy()` in `animating()`. G1 fixes the HUD early return.
  - Tests: hud.rs `the_frame_has_an_empty_track`. motion: `the_level_eases_without_overshoot`.

- [x] **7 [MED] Snap preview: rounded, accent tinted, glides from the window.**
  - Waits on: G2, G3.
  - Today render.rs:843-865: `thick = 3.0` (:848), four bars at `fill(.., 0.85)` (:862), fill at 0.12 (:864), white `plain`.
  - Extra bug: the preview uses the visible rect (render.rs:846, input.rs:533-539), but `land()` uses `work_zone()` (input.rs:393-414), which leaves out pinned bars. With a pinned bar, the window lands below where the preview showed.
  - Fix:
    1. input.rs: `fn snap_view(&self) -> Rect { let w = self.work_zone(); Rect::new(self.in_world(w.origin), w.size / self.camera.zoom) }`. Use it in `land()` (:412-413) and the preview (render.rs:846). Zone detection stays on the visible rect.
    2. `State.snap_art`, a `(2R + 1)` square, R = 12: `round_rect(full, R, accent, 0.22)` + `ring(full, R, 2.0, accent, 0.9)` (paint.rs:84, :184). Rebuild on accent change (follow.rs:283-286). Draw with `round_slices` (G3).
    3. `State.zone_glide: Option<(Rect, Instant)>`, `const SNAP_GLIDE: 180 ms`. In `drag()` (input.rs:533), when the zone changes to Some, `from` = the drawn preview rect, or the grabbed window frame when the zone was None. Clear it where `zone = None` (input.rs:578, 1262; windows.rs:795; heads.rs:81; lock.rs:153).
    4. Draw (move to a helper in windows.rs): `rect = between(from, to, smooth(t))`, alpha `smooth(t).max(0.35)`, then `modal_shadow` for contrast on light wallpapers. Reduce: `rect = to`, alpha fades.
    5. `zone_glide` busy term in `animating()`.
  - No setting.
  - Tests: snap.rs `the_preview_glides_from_the_window_into_the_zone`. render/tests.rs `the_snap_preview_matches_where_the_window_lands` (pinned bottom bar, `land(i, Zone::Left)`, frame equals `snap.rect(Left, snap_view())`).

- [x] **8 [MED] Focus change eases over 150 ms; inactive dim setting.**
  - Waits on: G2.
  - Jumps: shadow windows.rs:405 (`0.2` vs `0.105`), title dim windows.rs:353 (`1.0` vs `0.55`, inline literal), title art windows.rs:372 (`title-mid.png` vs `title-mid-idle.png`, window.rs:89-104). `awake` computed per frame at render.rs:1005.
  - Fix:
    1. shell.rs:50-71: `pub lit: f64` on `Window` (init 0 in `new_toplevel`, ~241-253). A level stepped toward the target handles fast alt-tab with no jump.
    2. drifter-core window.rs: `FOCUS_FADE: 150 ms`; `step_lit(lit, target, dt, pace)` (pace 0 gives target); `focus_look(lit, idle_title) -> (title, shadow)` with `e = smooth(lit)`, title `idle + (1 - idle) * e`, shadow `0.105 + 0.095 * e`.
    3. `step_windows` (windows.rs:24): step every `lit`. Fades stay under reduce.
    4. `animating()`: any `lit` not at its target.
    5. `Showing` (render.rs:141-150, :146): carry `lit: f32` in place of the `awake` bool; fill at render.rs:1005.
    6. `dress` (windows.rs:300-383): title alpha from `focus_look`. At :368-382 push the active piece at `fade * e`, then the idle piece at `fade * (1 - e)`; skip alpha 0. `cast_shadow` (:394-416) uses the shadow value.
  - Setting "Inactive windows" (Appearance, "Style" group, pages.rs:321-351):
    - config: `[window] inactive = "slight" | "more" | "none"`, default `slight` = today. In `WindowConfig` (config.rs:233-264).
    - Values: slight = title 0.55; more = title 0.4 + an 8% black wash over the client (upload a 1x1 black pixel, `fill` it after the client elements at render.rs:1063-1071, scaled by `1 - e`); none = 1.0.
    - Store in `Chrome.idle_title` via `Chrome::follow` (window.rs:174-184, called at follow.rs:663).
    - Model: flat `inactive` field (beside settings.rs:285-287), read ~462, set ~1777, write ~2138. `Id::Inactive`, `Segments [t("Slight"), t("More"), t("None")]`.
  - Tests: window.rs `the_focus_fade_lands_on_the_old_numbers`, `focus_fades_over_150ms_and_snaps_when_off`. render/tests.rs:427 must still pass.

- [x] **9 [LOW] Title bar detail fades over a zoom range.**
  - Waits on: nothing.
  - render.rs:122 `DETAIL_ZOOM = 0.75`; windows.rs:311 gates buttons (:321) and title (:343). Overview zoom is 0.7 (camera.rs:10), so every flight pops at 0.75.
  - Fix: render.rs:122 `const DETAIL_FADE: (f64, f64) = (0.72, 0.85);`. drifter-core window.rs `pub fn detail_alpha(zoom, (lo, hi)) -> f32` (smoothstep). windows.rs:311: filter `detail > 0`, multiply button alpha (:337) and title dim (:353).
  - Frames already run during flights (`flight.is_some()`, render.rs:685). Hidden buttons are never clicked: the overview takes presses below zoom 1 (input.rs:677). No setting.
  - Test: window.rs `chrome_detail_fades_over_a_zoom_range` (0 at 0.70, 1 at 1.0, between at 0.78, never falls).

- [x] **10 [MED] Dock icons lift on hover; running dots fade; Dock hover setting.**
  - Waits on: G2 (Magnify also waits on 18).
  - Done: Off and Lift, dot fades, the Hover row. Magnify is added with item 18.
  - Only the Apps logo reacts (dock.rs:713, docklogo.rs:44-68, `animating()` `dock_logo.hot` at render.rs:696). Other icons: dock.rs:719-721 at alpha 1. Dots: dock.rs:731-743 and Settings dot :751-754 at alpha 1. Icons are separate elements already.
  - Fix (Lift):
    1. core dock.rs after `BOUNCES` (:443): `HOVER_LIFT 4.0`, `HOVER_SECONDS 0.15`, `DOT_SECONDS 0.2`; `hover_lift(rising, p)` (ease up, smooth down, negative = up); `step(value, goal, dt, seconds)` (as docklogo.rs:61-65); `enum DockKey { App(String), Window(u64), Settings, Trash }`; `enum DockHover { Off, Lift, Magnify }`.
    2. State after `bounces` (render.rs:557, init start.rs:533): `dock_hover: HashMap<DockKey, (f64, bool, Instant)>`, `dock_dots: HashMap<String, (f64, Instant)>`, `dock_hover_mode`, `dock_magnify`.
    3. render/dock.rs `fn wind_the_dock(&mut self)`: hovered slot from `self.dock.at(..)` (as :713); goal 1 for it, 0 for the rest; step dots toward running or not; scale `dt` by pace. Call it in `draw_head` next to `settle_bounces()` (render.rs:1288).
    4. dock.rs:685 and :709: `lift = bounce_of(..) + hover_of(..)`. Same offset for minimized (:725-727), cog (:745), trash (:762).
    5. dock.rs:740 and :753: dot alpha from `dock_dots`.
    6. `animating()`: true only while a hover or dot value is in transit.
    7. Off, pace 0 or reduce: no lift. Dots still fade under reduce.
  - Magnify: per-icon elements exist. Needs icons rasterized at `icon * magnify` (follow.rs:693-716) and drawn smaller at rest, else blur. Add `Dock::magnified(band, pinned, minimized, pointer_x, amount) -> Vec<(Slot, Rect)>` beside `slot_rect` (dock.rs:95-108): width `icon * (1 + (amount - 1) * cos falloff(|dx| / (icon * 2.5)))`, bottom anchored, recentred by `start()` (:89-93). Draw only; keep hit tests on the rest layout. Panel width then moves each frame, so Magnify depends on item 18.
  - Setting: `[dock] hover = "off" | "lift" | "magnify"` (default lift), `magnify: u32` 110..=200 % (default 150, check in `validate`). In `DockConfig` (config.rs:402-445). Rows after `DockFit` (settings.rs:1203): `Id::DockHover` `Segments [Off, Lift, Magnify]`; `Id::DockMagnify` `Slider { 110, 200, "%" }.enabled(hover == Magnify)`. Model flat fields (settings.rs:295-298), read ~470, set ~1856, write ~2167. Reload in `apply` (follow.rs:683-707).
  - Tests: core dock.rs `a_hover_lifts_a_few_pixels_and_settles`, `the_magnified_icon_under_the_pointer_is_biggest_and_the_row_stays_centered`, `magnify_at_1_matches_slot_rect`.

- [x] **11 [MED] Power alert grows in and drains a bar; countdown setting.**
  - Waits on: 1.
  - Done with Option A: a static message and one timer.
  - The alert is in render/deskmenu.rs (`Alert` :38-46, `ask_alert` :564-584, `tick_alert` :586-601, `paint_alert` :603-636, `alert_elements` :676-683) and core alert.rs (`Look::paint` :92-136). `COUNTDOWN = 60` (core power.rs:53). It repaints the frosted art each second (deskmenu.rs:595-598).
  - Fix:
    - Grow-in via item 1: `Pane::Alert` with its own `ALERT_FROM_SCALE 0.90`, `ALERT_IN 0.22`; `drawer::ease` at Full, `smooth` at Calm; alpha only under reduce.
    - alert.rs: `pub fn drain(&self) -> Rect { Rect::new(Vec2::new(20.0, self.height - 64.0), Vec2::new(self.width - 40.0, 4.0)) }` (y 140 with the tick, 104 without; fits between text and buttons). Paint the track in `paint`: `round_rect(drain, 2.0, self.off, 1.0)`.
    - Fill on the GPU: `accent_pill` texture (drain size, accent, radius 2) on `Alert`. In `alert_elements`, `left = 1 - since / countdown`, push `faded(pill, (0, 0, w, 4), (rect.origin + drain.origin, (w, 4)), a)` before the art.
    - Frames: in `waiting()` before :734, `if self.alert.is_some() { return Some(EBB_TICK) }` (50 ms, render.rs:101). The bar moves about 5 px/s.
    - Pace 0: draw the bar full and static.
  - Owner decision: the message text. Option A (recommended): static "Your computer will restart automatically." Drop the per-second repaint (deskmenu.rs:595-598); one `Timer` fires the confirm at 0. Option B: keep the number for screen readers; the text still changes once a second.
  - Setting "Countdown": `[power] countdown: u64` 10..=60, default 60 (validate). `Power` (config.rs:895-904) needs a manual Default or a serde default fn. Row in the Power page unnamed group (pages.rs:870-918): `Id::Countdown`, `Slider { 10, 60, " s" }`. Set near pages.rs:1114, write at :1389-1398. Replace `COUNTDOWN` at deskmenu.rs:571 and :590 with `self.power_rules.countdown` (render.rs:446, set at follow.rs:629).
  - Tests: alert.rs `the_drain_bar_sits_between_the_text_and_the_buttons` (both looks). render/tests.rs `the_power_alert_drains`.

---

## Tier 2: about a day each

- [ ] **12 [MED] GPU hover highlight that slides between rows.**
  - Waits on: G3, 1, 5.
  - No hover today: Launchpad (`Pad`, render/launchpad.rs:7-15), Spotlight (only the keyboard row, search.rs:282-285; `hover()` render.rs:1446-1462 never asks search), Control Center (`Panel`, centers.rs:4-10), notifications (hover only extends expiry, overlays.rs:404-411).
  - Glance already has a hover, but on the CPU on the input path: `hover_glance` (activity.rs:169-215) repaints the panel from `drag()` (input.rs:515); painted at glance.rs:188-190. Convert it.
  - Row rects in screen space:
    - Launchpad: `look.slot_at(size, p - origin, on_page)` (launchpad.rs:167-169) then `cell_rect` (:148-157) + `pad.rect.origin`.
    - Spotlight: `look.row_at` (search.rs:230-232), `row_rect(i)` (:223-228) + rect origin; skip the selected row.
    - Control Center: new `pub fn hit_rect(&self, inside) -> Option<Rect>` in control.rs from `tile_rect` (:142) / `slider_rect` (:155).
    - Glance: make `row_rect` (glance.rs:85) pub, + `panel.rect.origin`.
    - Notifications: `card_shown()` (item 5).
  - Fix, new file `crates/drifter/src/render/lit.rs`:
    - `struct Lit { pane: Pane, from: Rect, to: Rect, since: Instant, alpha: (f64, f64, Instant) }`; `LIT_SLIDE 0.12`, `LIT_FADE 0.10`, `LIT_ALPHA 0.07`.
    - `fn hover_rows(&mut self)`, called from `hover()` (render.rs:1446). On a new target: `from = drawn rect`, `since = now`. On none: fade out. No paint call.
    - `fn lit_rect(&self) -> Option<(Rect, f32)>`: `between(from, to, smooth(t))`. Reduce or pace 0: jump to `to`, crossfade alpha.
    - Push with `round_fill` (G3, radius 8-9) just before each panel's art: cards render.rs:824, search :867, launchpad :880, control :886, glance :899. Multiply by the item 1 fade and scale.
    - Glance: drop `hover` from `glance::Paint` and `paint_glance` (activity.rs:130, 149). `hover_glance` sets `lit` and repaints only when `over_open` changes.
    - `lit` is cleared when its pane closes (check in `lit_rect`). `lit_moving()` in `animating()`.
  - Tests: control.rs `hit_rect`; glance.rs `row_rect` (tests at :265-299); render/tests.rs: Spotlight with results, pointer over row 1, `state.lit` targets `row_rect(1)` and no repaint lane was claimed.

- [ ] **13 [LOW] Spotlight height eases.**
  - Waits on: G2.
  - It jumps when the result count changes. `Look::size` (search.rs:208-214), `paint_search` (render/search.rs:58-95) sets rect at :78, swaps art at :89-92, drawn at render.rs:867-870. Origin is fixed (search.rs:216-221).
  - Fix: `Search` (render/search.rs:4-11) gets `height: (f64, Instant)`, `prior: Option<(TextureBuffer<Tex>, Rect)>`, `SEARCH_EASE 0.16`. In the done closure, record the drawn height and keep the old art when shrinking. Draw in two slices to keep the bottom corners (radius 16, search.rs:246): top `faded(art, (0, 0, w, h - r), ..)`, bottom `faded(art, (0, art_h - r, w, r), (o + (0, h - r), (w, r)))`. Use `prior` while `h > new height`. `modal_shadow` on `(o, (w, h))`. `search_moving()` in `animating()`. Pace 0: no ease.
  - Test: search.rs `pub fn slices(art, h, r)`: two slices cover exactly `h`, bottom comes from the last `r` rows.

- [ ] **14 [MED] Launchpad: zoom in, page slide, touchpad swipe.**
  - Waits on: G2.
  - Pages exist (`per_page`, `pages`, launchpad.rs:135-142; `Pad.page` render/launchpad.rs:11). Keys turn pages (:113-114). Wheel uses only vertical and drops `|steps| < 0.5` (:180-198, input.rs:1142-1150), so a two-finger scroll rarely turns a page. Dots are not clickable. 3+ finger swipes pan the camera even with the launchpad open (input.rs:1170-1190). The panel is one CPU image (launchpad.rs:204-209).
  - Fix:
    1. `Pad` gets `opened: Option<Instant>` (set when art first lands, :85-89), `turn: Option<(TextureBuffer<Tex>, Instant, i8)>`, `turning: i8`, `swipe: f64`. Consts `PAD_ZOOM 0.25`, `PAD_TURN 0.28`, `SWIPE_PAGE 90.0`.
    2. Zoom in: split the draw at render.rs:880-885 into the header band (`field_rect`, launchpad.rs:144-146) at 1:1 and the grid band scaled `0.9 + 0.1 * ease(t)` about its centre. Reduce: alpha only. (Per-icon zoom needs separate icon textures; not in this item.)
    3. Page slide: set `turning = ±1` where `page` changes (:113-114, :190-195). In the done closure keep the old art in `turn`. Draw the grid band (dots band fixed) as two crops sliding by `s = w * smooth(t)`. Same frost, so no seam. Reduce: crossfade.
    4. Swipe: in the `PointerAxis` branch (input.rs:1129), when the launchpad is open and the source is `AxisSource::Finger`, add the horizontal amount to `swipe`; turn at `|swipe| >= SWIPE_PAGE`; reset on a stop event; do not forward to clients. In `GestureSwipeUpdate` (:1174) add `delta_x` to `swipe` instead of panning; decide in `GestureSwipeEnd` (:1190).
    5. `pad_moving()` in `animating()`.
  - No setting for the swipe.
  - Tests: launchpad.rs `pub fn page_after_swipe(page, pages, swipe, threshold) -> (usize, f64)` (threshold, clamp at ends); slide crop widths sum to `w`. render/tests.rs: two pages of apps, swipe, `page == 1`, `turn.is_some()`.

- [ ] **15 [LOW] Switcher selection slides between tiles.**
  - Waits on: G3.
  - It is a filled tint, not a ring: `canvas.round_rect(tile_rect(chosen), 12.0, look.pick, look.pick_alpha)` (switcher.rs:100; pick = shade, 0.16, :33-34). Each step repaints the panel (`switch_step`, overlays.rs:4-43). The title under the tiles still needs the repaint.
  - Fix: `Switch` (render.rs:262-269) gets `ring: (Rect, Instant)`, `RING_SLIDE 0.14`. Set on open (overlays.rs:27-34) and on step before `open.at = at` (:39-41) from the drawn rect. switcher.rs: `paint(chosen: Option<usize>, ..)`, pass `None`; expose `pick_style()`. In `switch_elements` insert at overlays.rs:475 (between icons and art) a `round_fill` (radius 12, alpha 0.16) at the lerped rect. Wrap-around jumps more than one tile: snap. Reduce: fade at target. `switch_moving()` in `animating()`.
  - Test: switcher.rs `the_panel_paints_without_a_pick_when_none_is_chosen`; update `same_at_two` (:157-163).

- [ ] **16 [BUG] A window closed by its app vanishes, and the close button's fade is cut short.**
  - Waits on: G2.
  - `toplevel_destroyed` (shell.rs:257-260) calls `window_gone` (windows.rs:779-809), which removes the window at :787 with no check of `going` or `motion`.
  - The close button starts a 150 ms fade (input.rs:456-464), but most apps destroy the toplevel in one or two frames, so `window_gone` cuts it short. Also no motion on: window list close (bars.rs:205), dock menu close (dock.rs:540-542), app-name Quit (appname.rs:141-144).
  - No snapshot exists: closing draws the live surface (render.rs:1025-1033).
  - Fix:
    1. State `ghosts: Vec<Ghost>` (near render.rs:556): `struct Ghost { art: TextureBuffer<Tex>, frame: Rect, dressed: bool, motion: Motion }`.
    2. windows.rs `fn snapshot_window(&mut self, index) -> Option<(TextureBuffer<Tex>, Rect)>`: build client elements (render.rs:1018-1033) + `dress(..)`, wrap in `RelocateRenderElement` by `-frame origin`, render with `OutputDamageTracker` into `self.renderer.canvas(..)` (paint.rs:240-253), as capture.rs:107-123. GLES: `TextureBuffer::from_texture(.., Tex::Gles(tex), ..)` (as wallpaper.rs:164-170). Pixman: `copy_framebuffer` + `upload_at` (capture.rs:124-136). Pixman is the test path, so both are needed.
    3. `window_gone` before :787: skip if `!placed`, `hidden` or already faded (`closed_at`). Else snapshot, keep an in-flight Closing motion or start `Motion::new(Phase::Closing, rect)`, push a ghost.
    4. `step_windows`: step ghosts with the windows.rs:30-33 pace, drop when done. `!ghosts.is_empty()` in `animating()`.
    5. Draw before the windows loop (render.rs:~1013, on top): `motion.shape_for(frame, reduce)`, `element(art, on_screen(shown), a)`, `cast_shadow(shown, dressed, false, a)`.
    6. Later: GTK may commit a null buffer before destroy (`settle_window` returns early, windows.rs:435-437). Snapshot on that commit in `commit` (shell.rs:100, before :102) and stash it for `window_gone`.
  - No setting.
  - Test: render/tests.rs `a_window_that_quits_fades_out_instead_of_vanishing`: `red_window` (tests.rs:239), stop the client, `ghosts.len() == 1` and the old frame has non-background pixels; after 150 ms, empty. `other_apps_see_each_window_come_and_go` (tests.rs:914) must still pass.

- [ ] **17 [MED] Fullscreen glides in and out.**
  - Waits on: G2.
  - `enter_fullscreen` (windows.rs:610-650) moves and resizes with no motion; the chrome drops at once (render.rs:1006 `dressed = decorated && full.is_none()`). `leave_fullscreen` (:655-670) sets `content = back` at :662 with no motion.
  - Reuse: `Phase::Resizing` (`between(dock, window, smooth(t))`, motion.rs:75, 230 ms), as maximize does (input.rs:494-499).
  - Fix:
    1. motion.rs: `Phase::Filling`, `Phase::Unfilling` (enum :7-13), span `RESIZE`, Resizing shape. `pub fn chrome(&self) -> f64`: Filling `1 - smooth(t)`, Unfilling `smooth(t)`, else 1.
    2. windows.rs:627-633: `let was = window.content; window.content = screen;` and if no motion, `Motion::new(Filling, was)`.
    3. windows.rs:655-662: `let Some((back, screen)) = window.full.take()`; after `content = back`, `Motion::new(Unfilling, screen)`.
    4. render.rs:996-1006: dress while Filling; pass `fade * motion.chrome()` to `dress` (:1056) and `cast_shadow` (:1073); add the factor to `Showing` (:141-150).
  - Tests: motion.rs `filling_glides_like_a_resize_and_fades_the_frame`. render/tests.rs `fullscreen_glides_in_and_back_out`.

- [ ] **18 [MED] The dock glides when apps open, close or minimize.**
  - Waits on: 10.
  - `slot_rect` (core dock.rs:95-108) is a pure function of counts, recomputed every frame (dock.rs:664-667, 686, 710). With `dock.fit` on (default off, config.rs:425-426), `fit_dock` (bars.rs:655-701) repaints the panel on a worker and swaps it, then `frost_key = None` (:697) repaints again: two jumps.
  - Fix:
    1. core dock.rs: `GLIDE 0.22`, `glide(from, to, t)` (ease; smooth when no bounce).
    2. State: `dock_slides: HashMap<DockKey, (f64, f64, Instant)>`, `dock_span: (f64, f64, Instant)`, `dock_ghosts: Vec<(TextureBuffer<Tex>, Rect, Instant)>`.
    3. `dock_elements` (dock.rs:648): for every slot (cog, trash, dividers too) retarget on change from the shown x. New keys fade in at the target. Gone keys (dock.rs:52-70) become ghosts that fade over `GLIDE`. Badges, dots and the logo use the shown x. Drag reorder (dock.rs:83-92) glides for free.
    4. Panel width (fit only): keep target and shown spans apart. Paint the panel at `max(shown, target)` in `fit_dock` (bars.rs:657-662) and `frost_bars` (wallpaper.rs:475) via a shared `fn dock_paint_shape(&self) -> Panel`. Draw it as 3 crops (`cropped`, render.rs:1838) in place of render.rs:963-968: left cap, centred unstretched middle, right cap. Wrap shadow (render.rs:970-975) at the shown width. Repaint at the target when the glide ends.
    5. Busy terms in `animating()`. Pace 0 or reduce: snap, ghosts still fade.
  - No setting.
  - Tests: core dock.rs `a_glide_starts_at_from_and_ends_at_to`, `the_row_recentres_by_half_a_step_when_one_app_opens`; drawer.rs `panel_slices` cover the width with no stretch.

- [ ] **19 [MED] Settings and Activity get Explorer's Fade and Glide.**
  - Waits on: G2.
  - Explorer: `Glide` and `Fade` (explorer window.rs:64-78), `crop`/`paste` (:80-104), fields :147-148, consts :52-54, fade draw :608-626 and :654-659, swap crossfade :667-675, glide tick :737-763 (from loop :372-375), wheel :1113-1156 (discrete wheel only, :1115). `explorer::look::ease` (look.rs:316) equals `motion::smooth`.
  - Settings and Activity have no UI transitions (the About logo and Activity's hold-to-force bar are the only motion).
  - Fix:
    1. New `crates/drifter-core/src/tween.rs` (`pub mod tween;` in lib.rs between :60 and :61; `motion` is taken). API: `ease(t)`; `Glide { from, to, shown, since, span }` with `new`, `retarget(to, now)`, `step(now, dense) -> Option<f64>` (pixel snapped), `done`; `Fade { area, at, old, new, since, span }` with `new`, `old_alpha`, `done`; `crop(image, area, dense)`; `paste(image, at, piece)`.
    2. Explorer: delete window.rs:64-104; use `tween::*`; `look::ease` calls `tween::ease`.
    3. Settings (drifter-settings window.rs). Rule: animation state never enters `Model`, `groups()` or `Look` (except `side_scroll`); the fingerprint (:1167-1187) would force a full repaint each frame.
       - Wheel glide: axis :3001 to `scroll_by` (:2377-2399). Discrete wheel with smooth on sets `self.glide`; clamp with `max_scroll` (:1251-1255). Step in the loop (:398-411) before `draw_due`, set `stale`; add `|| glide.is_some()` to the timeout at :401. Frames stay cheap: same fingerprint, so draw composes cached tiles (`compose_pages` :1221-1249, `compose_side` :1189-1219). Prewarm the target tile at glide start. Sheets have their own tile cache (:910-944).
       - Page crossfade: in `go_to` (:2440-2446) keep `self.last`; after the new page paints, `Fade` over the content area (right of the sidebar); compose without repaint (as Explorer :609-622).
       - Switch animation: move the switch painter (settings.rs:3459-3476) into `Look::paint_switch(canvas, pill, t, fade)` (lerp colour and knob x). On `Hit::Change(id, Switch)` (act :1286-1293) record `flip`; for 150 ms repaint only `switch_rect` over `last`. Add partial damage (as Activity window.rs:433-443); today `show()` damages the whole buffer (:1155).
       - Draw rate: `draw_due` caps at 100 ms without a frame callback (:813-818, :2793-2801); the loop wakes every 16 ms only while `stale || unsaved` (:401). Keep `stale` set while animating.
    4. Activity (drifter-activity window.rs): wheel glide at axis :883 / `scroll_by` :654-665, tick sets `model.scroll`, `scrolled_only = true`, `stale = true` (cached tall layer, :377-386). View crossfade on `Hit::View` (:559-568). `watch` (:313-339) must also store smooth, reduce, animation.
  - Setting "Smooth scrolling": `[mouse] smooth_scroll: bool`, default true, in `Mouse` (config.rs:671-699). Row in the Mouse group (pages.rs:262-292): `Id::SmoothScroll`, Switch. Set at pages.rs:1077-1080, write array :1310-1318. Each app computes `smooth = smooth_scroll && !reduce && animation != Off`. Explorer reads drifter.toml at window.rs:190-191 and `watch` (:492-515, stores only appearance today); gate the glide at :1136 and :1141.
  - Tests: tween.rs `a_glide_lands_on_its_target_and_snaps_to_pixels`, `retarget_starts_from_what_is_shown`, `a_fade_runs_old_alpha_from_1_to_0`. Explorer's tests must still pass.

- [ ] **20 [LOW] One menu style.**
  - Waits on: nothing.
  - Panel menus `menu::Look::themed`: rows 24 (menu.rs:91), radius 8 (:93), hover radius 5 (:214), label x 10 (:229), back 0.965 (:99). Desk menu `cascade::Look::themed`: rows 26 (cascade.rs:97), radius 10 (:100), hover radius 6 (:203), label x 13 (:210), back 0.96 (:104).
  - Recommendation (owner to confirm): the desk menu style wins. Radius 10 matches window chrome (window.rs:28) and drawers (drawer.rs:43). 26 px rows fit its 18 px icons.
  - Fix: menu.rs `pub const ROW: f64 = 26.0; pub const RADIUS: f64 = 10.0; pub const HOVER_RADIUS: f64 = 6.0;`. Use them at menu.rs:91, :93, :214 and cascade.rs:97, :100, :203. Also match label x (menu.rs:229 to `pad + 8.0`), rule colour and inset (menu.rs:204 to `pad + 6.0`, palette rule colour), back alpha (menu.rs:99 to 245). Keep the minimum widths.
  - Test: menu.rs `panel_and_desktop_menus_share_one_style` (item and radius equal).

- [ ] **21 [LOW] Hard-coded colours and labels.**
  - Waits on: nothing.
  - All three are in `crates/drifter/src/tray.rs` (the D-Bus host), not core tray.rs.
  - Tray fallback icon: `placeholder()` (tray.rs:173-183) paints gray `[190, 196, 210]` / `[60, 64, 76]`, returned with `symbolic = false` (:162), so `take_icons` (bars.rs:31-35) never tints it. Fix: add `pub placeholder: bool` to `tray::Icon` (tray.rs:14-19), paint it white, tint with `self.look.accent` in `take_icons`. Re-take icons when the accent changes (follow.rs:283-286; `recolor` only re-takes on palette change, :403).
  - Hot corner glow: `paint_glow(REACH, [255, 214, 160])` (start.rs:346-347), never repainted, drawn at render.rs:834-841.
    - Extra bug: it shows and keeps a TICK (render.rs:754) when Hot corner is off. Only camera.rs:41 checks `tunables.hot_corner`.
    - Extra bug: it uses fixed `corner::REACH` 128 (corner.rs:4), not `corner_size`.
    - Fix: `fn paint_corner_glow(&mut self)` with the corner size and `self.look.accent`; call at start.rs:346 and on accent change. Gate render.rs:834 and :754 on `tunables.hot_corner && corner_glow`.
    - Optional setting "Glow": `[motion] corner_glow: bool` (default true, config.rs ~710, add to `Tunables` :1213-1268). Row in the Hot corner group before pages.rs:96: `Id::CornerGlow`, `Switch.enabled(motion.hot_corner)`. Set near :1081, write in the flag list :1321-1329.
  - Tray tooltip: tray.rs:163 falls back to `service.to_string()` (e.g. `:1.87/StatusNotifierItem`). `tip()` (:185-205) already tries ToolTip title, body, `Title`, `Id`. Fix: pure `fn label(tip, title, id, desktop, comm, service) -> String`. Order: ToolTip title, `Title`, .desktop Name for `Id` (`crate::apps::find`, apps.rs:9), .desktop Name for the process, prettified `Id` (`-`/`_` to spaces, capitalised), process name (`GetConnectionUnixProcessID` then `/proc/<pid>/comm`), service string last. `read_icon` runs on the tray thread, so file reads are fine.
  - Tests: tray.rs tests (:575-587) `a_tray_tip_never_shows_a_bus_name`. corner.rs `the_glow_is_painted_in_the_given_color`.

---

## Tier 3: shaders

Shared: smithay 0.7.0 has `compile_custom_texture_shader(src, &[UniformName])` (gles/mod.rs:1969) and `render_texture_from_to(.., alpha, Some(&program), &uniforms)` (gles/mod.rs:2488). Nothing uses it yet. The source must contain the line `//_DEFINES_` and handle `EXTERNAL`; inputs are `tex`, `v_coords`, `alpha`. Pixel shaders cannot sample a texture. Put each program next to `overlay_program` (wallpaper.rs:222-242), its GLSL as a const in core, and render into a `Sheet`. Pixman always falls back.

- [ ] **22 [MED] Right-click ripple on the GPU; ripple setting.**
  - Waits on: G2.
  - Today: a `ripple` thread (deskmenu.rs:191-214) steps a 300x300 wave grid every 16 ms (:193, :212), and each frame spawns up to 8 scoped threads (ripple.rs ~171-205) and uploads a new texture on the main thread (deskmenu.rs:237). `wallpaper_patch` (deskmenu.rs:128-181) renders the wallpaper only (no windows) and reads back a 600x600 region once per click (:157-161), then downscales, so it is blurry at 2x. It always runs (deskmenu.rs:112) and ignores reduce motion.
  - Fix:
    1. Rename `wallpaper_patch` to `fn wallpaper_snapshot(&mut self) -> Option<GlesTexture>`: keep the render (:136-149), return the `Canvas::Gles` texture, delete the readback (:150-180).
    2. `ripple_program` in wallpaper.rs; GLSL `drifter_core::ripple::GLSL`. Uniforms `S` (screen px), `O` (click px), `K` (seconds), `G` (strength). Ring wave `sin(x * f) * gauss(x) * exp(-K * damp) * G`, radial offset with per-channel dispersion 0.94 / 1.0 / 1.07 and `BEND` (ripple.rs:76-77), plus a light shade term.
    3. `Ripple` (deskmenu.rs:31-36) becomes `{ snapshot: GlesTexture, sheet: Sheet, area, at, since, art }`. `wind_the_ripple` (:225) renders the snapshot through the program into a full-screen sheet, damage = the ring's box. Whole screen, native density.
    4. Check the offscreen texture's y orientation with an off-centre click on the device.
    5. Delete `Water`, `bilinear`, `CELL`, `STEPS_PER_SECOND`, `DROP`, `SPONGE` (ripple.rs:45-235) and the dead `ripple::capture` (ripple.rs:16-43, no caller).
  - Setting: `[workspace] ripple: bool` (default true), optional `ripple_strength: u32` 25..=200 %. Wallpaper page first group (settings.rs:1598-1608): `Id::Ripple` Switch, `Id::RippleStrength` Slider `.enabled(ripple)`. Flat model fields like `drift` (settings.rs:308, 493, 1760, 2015). Gate deskmenu.rs:112 with `ripple && !window_still && window_pace > 0 && !is_pixman()`.
  - Tests: ripple.rs `the_ring_reaches_the_screen_corner_before_it_fades` (`SPEED * SECONDS >= 1920x1080 diagonal`), `the_ripple_shader_names_its_inputs` (mirror shader.rs:555-569).

- [ ] **23 [MED] Circular reveal for Dark/Light and wallpaper changes.**
  - Waits on: G2, 22.
  - Bug first: a manual theme change never animates. Control Center (`flip_tile`, centers.rs:243-247) and Settings only write the config. `watch_config` (follow.rs:472-489) calls `apply`, which calls `follow_looks` at once (follow.rs:633). `watch_style` runs after it (start.rs:904-907) and sees no change. So the 2 s fade (`style_fade_elements`, follow.rs:557-569, `STYLE_FADE` render.rs:110) plays only on Auto at dusk and dawn. Reduce motion and Pixman skip even that (follow.rs:496).
  - Snapshot today: `shoot_old_style` (render.rs:1323) calls `capture` (capture.rs:100-137), a full GPU to CPU to GPU round trip, into `style_shots` (follow.rs:521-535).
  - Wallpaper change already crossfades over 1.5 s (wallpaper.rs:390-391, render.rs:1085-1103).
  - Fix:
    1. `apply` before follow.rs:633: when `theme::now(style) != self.style`, animation is on and not Pixman, set `restyle_due = Some(Box::new(config.clone()))`, clear `style_shots`, `style_fade = None`, and skip `follow_looks`; `restyle` applies it after the shot. Else call `follow_looks` as today. Remove the reduce check at follow.rs:496 (G2 step 8).
    2. capture.rs: `capture_texture(&mut self, shown) -> Result<GlesTexture, String>` (capture.rs:100-123 without the readback). In Reveal mode on GLES, `shoot_old_style` keeps `style_tex: Vec<(usize, GlesTexture)>`.
    3. New file `crates/drifter/src/render/reveal.rs`: `reveal_program` (uniforms `S`, `O`, `Q` radius px, `F` feather px; `gl_FragColor = texture2D(tex, v_coords) * alpha * smoothstep(Q - F, Q + F, length(v_coords * S - O))`). `fn paint_reveal(&mut self)` renders the old shot through it into a full-screen sheet each frame. `Q = smooth(t) * farthest corner distance from O`. `const REVEAL: 600 ms` by `STYLE_FADE`.
    4. `style_fade_elements` (follow.rs:557-569): in Reveal, insert the sheet at the same index (:566-568). `settle_style_fade` (:548-555) uses `REVEAL`.
    5. Click point: Control Center stores `reveal_at = Some((self.pointer, now))` at `flip_tile` (centers.rs:243). A Settings change uses `self.pointer` when `watch_config` sees it. Auto flips and folder rotation use the screen centre.
    6. Wallpaper reveal: in `take_wallpaper` (wallpaper.rs:379-398), render the old paper once to a texture (reuse the render half of `wallpaper_snapshot`, item 22), store `paper_reveal: Option<(GlesTexture, Vec2, Instant)>`, mask it the same way, draw where the old paper draws (render.rs:1099-1103). Keep it short; panning during it misaligns the shot.
  - Setting "Theme change": `[appearance] transition = "fade" | "reveal"`, default fade, in `Appearance` (config.rs:842-874). Appearance "Style" group (pages.rs:321-351): `Id::ThemeChange`, `Segments [t("Fade"), t("Reveal")]`. Set near pages.rs:1101, write :1344-1377. Effective mode is Fade when reduce or Pixman.
  - Tests: motion.rs `pub fn reveal_radius(at, screen, t)` and `the_reveal_reaches_the_farthest_corner_at_the_end`. render/tests.rs: flip `appearance.style` through `apply`, assert `restyle_due` is set, not applied at once. Settings round trip for `ThemeChange`.

- [ ] **24 [MED] Genie minimize; Minimize effect setting.**
  - Waits on: 16.
  - Today: `Motion::new(Phase::Minimizing, seat)` (input.rs:466-471, appname.rs:126-127), slot from `dock_slot` (windows.rs:8-22), straight lerp with alpha `1 - t²` (motion.rs:67-74). The window is the live surface through `RescaleRenderElement` (render.rs:1014-1017, 1064), which cannot bend.
  - Fix:
    1. Snapshot window + chrome once at minimize start with `snapshot_window` (item 16). Store `Window.stow_art`. Restore takes a fresh snapshot.
    2. New `crates/drifter-core/src/genie.rs`: `pub const GENIE_FRAG` and `pub fn source_of(p, win, slot, t) -> Option<Vec2>` (CPU mirror for tests). New `crates/drifter/src/render/genie.rs`: lazy `State.genie: Option<GlesTexProgram>` with uniforms `win` (vec4), `slot` (vec4), `t` (float).
    3. GLSL, per-pixel inverse map, one draw, no mesh. `vec4` is `(x, y, w, h)` in 0..1 of the box:
       ```glsl
       #version 100
       //_DEFINES_
       #if defined(EXTERNAL)
       #extension GL_OES_EGL_image_external : require
       #endif
       precision mediump float;
       #if defined(EXTERNAL)
       uniform samplerExternalOES tex;
       #else
       uniform sampler2D tex;
       #endif
       uniform float alpha;
       varying vec2 v_coords;
       uniform vec4 win;
       uniform vec4 slot;
       uniform float t;
       void main() {
         vec2 p = v_coords;
         float bend = smoothstep(0.0, 0.45, t);
         float slide = smoothstep(0.25, 1.0, t);
         float top = mix(win.y, slot.y, slide);
         float bot = mix(win.y + win.w, slot.y + slot.w, slide);
         float k = clamp((p.y - win.y) / max(slot.y + slot.w - win.y, 1e-4), 0.0, 1.0);
         float c = 0.5 - 0.5 * cos(3.14159 * k);
         float l = mix(win.x, mix(win.x, slot.x, c), bend);
         float r = mix(win.x + win.z, mix(win.x + win.z, slot.x + slot.z, c), bend);
         vec2 uv = vec2((p.x - l) / (r - l), (p.y - top) / (bot - top));
         if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) discard;
         gl_FragColor = texture2D(tex, uv) * alpha;
       }
       ```
       Assumes a bottom dock (windows.rs:9). Mirror `y` for a top dock.
    4. Per frame while Minimizing or Restoring with Genie: `box = union(frame, slot)`; render the snapshot into a reusable sheet with `render_texture_from_to(.., Some(&genie), &[win, slot, t])`; push `element(&sheet.buffer, box, 1 - t²)`; no shadow. Restoring passes `1 - smooth(t)`. Branch in the render.rs:1013 loop before the client elements; skip the live client and `dress`. Consider 380 ms span for genie (STOW is 260, motion.rs:25).
    5. Fallbacks: Pixman or a failed compile use Scale. Reduce: `shape_for` fade in place.
  - Setting "Minimize": `[motion] minimize = "scale" | "genie"`, default scale. Row in the G2 Animation group: `Id::MinimizeEffect`, `Segments [t("Scale"), t("Genie")]`. Read into `State.minimize_effect`.
  - Tests: genie.rs `genie_at_rest_is_the_window`, `genie_ends_inside_the_slot`, `genie_narrows_toward_the_dock`. render/tests.rs `a_taskbar_sees_windows_and_can_minimize...` (tests.rs:1447) must pass on the Scale fallback.

- [ ] **25 [LOW] Wallpaper shaders react to the pointer; setting.**
  - Waits on: G2.
  - Uniforms today: `T`, `L`, `W`/`R`, `S0`, `S1`, `A`, `B`, `C` (HEAD shader.rs:16-30, names wallpaper.rs:43-48, values :181-186). `P = vec2(v.x, 1 - v.y) * R` (shader.rs:41). Files: shaders/caustics.shader:1-16, shaders/aurora.shader:1-20.
  - Low-redraw plan: `Live::advance` repaints the back sheet in 15 strips, one per frame, every `live_every()` (wallpaper.rs:126-153, 335-340; 250 ms / drift; Still = 10 s). Bare desktop test exists: bars.rs:311.
  - GPU strength: only `is_software` (start.rs:1007-1012). The GL renderer name goes to `DRIFTER_RENDERER` (start.rs:190), which Settings already reads (drifter-settings window.rs:196). `State` does not keep it.
  - Fix:
    1. HEAD (shader.rs:23-29): `uniform vec3 M;` (xy in P space, z strength). Register `UniformName::new("M", _3f)` (wallpaper.rs:43-48); `paint()` (:168) takes `pointer: [f32; 3]`.
    2. caustics after line 9: `vec2 m=vec2(M.x/R.y,M.y/R.y)*3.;vec2 dm=p-m;p+=M.z*.25*dm*exp(-dot(dm,dm)*1.5);`. aurora before line 17: `vec2 dm=(P-M.xy)/R.y;col+=mix(A,C,.5)*M.z*.35*exp(-dot(dm,dm)*30.)*mix(1.,.3,L);`. No `//` comments (shader.rs:565).
    3. core wallpaper.rs `pub fn pointer_in_paper(source, target, art, whole, pointer) -> Vec2` (y up). Use it in `paper_pieces` (render.rs:1758-1777).
    4. `Art` (wallpaper.rs:15-23) gets `shown: (f64, f64)` (the front's moment, set at :104-114 and :148-152). `Live::show` gets `pointer: Option<[f32; 3]>`; while set, repaint only the front sheet inside the union of the old and new `±POINTER_REACH` (240) box, at the front's moment. Generalise `rows` to `damage: Option<Rectangle>` (:188-201). On strength 0, one last patch with z = 0, then stop.
    5. State `paper_pointer: Option<(Vec2, Instant)>`, set in `hover()` (render.rs:1446) on a bare desktop when the setting is on, `drift > 0`, not reduce, no lock, no fullscreen. `z = smooth(1 - elapsed / 800 ms)`. Busy term in `animating()`.
    6. core shader.rs `pub fn strong_gpu(name: &str) -> bool`: true for nvidia, geforce, rtx, radeon rx, radeon pro, arc; false for intel, uhd, hd graphics, iris, mali, adreno, v3d, panfrost, unknown. Pixelbook reports "Mesa Intel(R) UHD Graphics 615". Store the name from start.rs:182 in State.
  - Setting "Reacts to pointer": `[workspace] wallpaper_pointer: Option<bool>`, None = `strong_gpu`. Wallpaper "Shaders" group beside `Id::Drift` (settings.rs:1682): `Id::WallpaperPointer`, Switch `.enabled(drift > 0)`, default from `strong_gpu(DRIFTER_RENDERER)`. Off when Still.
  - Tests: shader.rs `every_shipped_shader_can_read_the_pointer`, `strong_gpu_tells_the_pixelbook_from_the_rtx`. Core wallpaper.rs `the_pointer_maps_into_shader_space_with_y_up`. `a_still_has_the_asked_size_and_is_opaque` (shader.rs:572-579) must still pass.

---

## Settings summary

| Control | Key | Page / group | Item |
|---|---|---|---|
| Animation Off / Calm / Full | `[motion] animation` | Desktop / Animation (new) | G2 |
| Minimize Scale / Genie | `[motion] minimize` | Desktop / Animation | 24 |
| Position Top / Bottom | `[notify] position` | Notifications | 5 |
| Inactive windows Slight / More / None | `[window] inactive` | Appearance / Style | 8 |
| Dock hover Off / Lift / Magnify + size | `[dock] hover`, `magnify` | Dock | 10 |
| Countdown 10-60 s | `[power] countdown` | Power | 11 |
| Smooth scrolling | `[mouse] smooth_scroll` | Mouse | 19 |
| Glow | `[motion] corner_glow` | Desktop / Hot corner | 21 |
| Ripple + strength | `[workspace] ripple`, `ripple_strength` | Wallpaper | 22 |
| Theme change Fade / Reveal | `[appearance] transition` | Appearance / Style | 23 |
| Reacts to pointer | `[workspace] wallpaper_pointer` | Wallpaper / Shaders | 25 |

Recipe for each (from `Id::ReduceMotion` and `Id::Drift`): config field + serde default + Default (+ `validate` if ranged) in config.rs; document in conf/default.toml; `Id` variant (settings.rs:2517-2653); row in pages.rs or settings.rs; `set_more` arm (pages.rs:1029-1145) or `Model::set` (settings.rs:1760); `write_more` diff (pages.rs ~1300-1437) or `Model::write` (settings.rs:2015); read in `apply` (follow.rs:571-723) and init in start.rs; lang entries; round-trip test (pages.rs:1459-1467 `round_trip`).

## Owner decisions

1. Item 20: desk menu style wins (radius 10, rows 26). Recommended.
2. Item 11: alert text static (recommended) or keep the live number.
3. Item 8: "More" also washes inactive window content 8% dark. Today only the title dims.
4. Item 2: there is no window title-bar menu. This list treats "title bar" as the top-bar app-name menu. A real title-bar right-click menu is a new feature, not on this list.
5. G2: window speed no longer follows the wallpaper slider. Users at wallpaper motion 0..60 get Full until they pick.

## Reference (not work)

- Already animated: desk menu grow-in (deskmenu.rs:325-362), screenshot thumbnail fade and slide (shoot.rs:771-783), Apps logo hover (docklogo.rs:57-66), wallpaper crossfade 1.5 s (render.rs:1085-1103), Auto theme fade 2 s (follow.rs:491-569).
- Already shadowed: Spotlight, Launchpad, glance card, power alert, screenshot bar, desk menu.
- Glance already has a hover highlight; item 12 moves it to the GPU.
- Hidden title buttons in overview are not clickable: the overview takes presses below zoom 1 (input.rs:677).
- The panel close count is 109 sites, not 71. Item 1 needs none of them changed.
- The power alert lives in render/deskmenu.rs and core alert.rs. render/power.rs is unrelated.

## Coverage

- Read in full: render.rs, render/{windows, deskmenu, overlays, dock, docklogo, launchpad, search, centers, activity, worker, wallpaper}.rs, drifter/src/tray.rs, drifter-core/{window, motion, menu, launchpad, search, notify, switcher, hud, dock, shader, lang}.rs, activity/glance.rs, settings/pages.rs, shaders/{aurora, caustics}.shader.
- Read by range: render/{input, follow, start, bars, shoot, capture, appname, lock, power, art, camera, tests}.rs, shell.rs, paint.rs; drifter-core/{config, settings, welcome, cascade, alert, power, control, ripple, corner, drawer, paint, frost, snap, theme, lib}.rs; drifter-settings, drifter-explorer and drifter-activity window.rs; conf/default.toml; smithay 0.7.0 gles/mod.rs, gles/element.rs, texture.frag.
- Five read-only agents covered the items in groups. Their key claims were checked again against source (titles, focus numbers, DETAIL_ZOOM, window_gone, waiting(), switcher tint, glance hover, menu sizes, COUNTDOWN, modal_shadow, tray lines, glow colour, ripple thread, style fade gate, dock hover, Settings sections, Explorer Glide/Fade).
- After this list: work it per `INSTRUCTION-punchlist.md`, one commit per item, and rename to `DONE-` when every box is checked.
