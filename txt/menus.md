# Top-bar menus and panels: one visual language

Goal: every menu and panel that opens from the top bar looks and moves like one family. Control Center is the reference: the user likes its detail, so every other surface copies its material, rows, discs, highlight and motion. Today Control Center and Notification Center have rich rows, while the menus are plain lines of text.

Audit done on main `a89e1e9` (2026-10-22), with screenshots of each surface from the trixie VM. Line numbers drift: check them before each edit.

Work rules (same as txt/network.md):
- Build on 114 and the VM. Check each item live.
- No comments or documentation in our code. No new unit or render tests.
- Every new `lang::t` string needs es, de and fr lines.
- `render.rs` does not grow. New compositor code goes in new files.
- Motion: arriving uses `drawer::ease`; leaving and levels use `motion::smooth`. Divide all times by `window_pace`. Reduce Motion keeps fades only. Pace 0 is instant.

---

## 1. What is there today

| Surface | Opened by | Drawn by | Radius | Backing | Rows | Hover | Icons | Opens with |
|---|---|---|---|---|---|---|---|---|
| App menu (Drifter / app name) | click app name | `menu::Look::paint` (menu.rs:187) | 10 | 0.96 | 26 px text | accent bar, repainted | none | grow from click, 0.35 to 1, 0.2 s |
| Power menu | click power | same | 10 | 0.96 | 26 px text | accent bar | none | same |
| Wi-Fi left / right | click / right click | same | 10 | 0.96 | 26 px text, status lines as greyed items | accent bar | none | same |
| Bluetooth left / right | click / right click | same | 10 | 0.96 | 26 px text | accent bar | none | same |
| App tray menus (StatusNotifier) | click a tray icon | same, submenus as a "Back" row | 10 | 0.96 | 26 px text | accent bar | none: icons, check marks and radio dots are dropped (tray.rs:324-338) | same |
| Tooltips | hover | `menu::Look` with `least_width` 0 | 10 | 0.96 | one line | none | none | fade 0.16 s |
| Control Center | click toggles icon | `control::Look` (control.rs) | 14 | 0.96 | cards, 46 px device rows with discs | soft GPU highlight that slides (lit.rs) | yes | fade + scale 0.96, list springs |
| Notification Center | click clock | `calendar::Look` | 14 | 0.96 | calendar + note cards | none | letter avatars ("W", "B") | fade + scale 0.96 |
| Batteries | click battery / desktop icon | `charges` card | 14 | 0.62 | rows with rings | none | ring per battery | fade + scale 0.96 |
| Glance | hover activity dot | `glance::Card` | 14 | 0.55 | 22 px rows, sparkline | soft highlight | none | fade + scale 0.96 |

## 2. What is wrong

1. Two families. Menus are radius 10 text lists. Panels are radius 14 card layouts. They sit side by side in the same bar.
2. Three backings: 0.96, 0.62 and 0.55. Panels next to each other read as different materials.
3. Two hover styles. Menus repaint the whole image with a hard accent bar on every hover change. Panels use a soft highlight that glides on the GPU.
4. No icons in any menu, so menus are hard to scan. Wi-Fi and Bluetooth rows have no disc, bars, lock or battery, while the same items in Control Center have all of them.
5. Status lines (Wi-Fi right click) look like disabled actions: grey text with no structure. Key and value are run together ("Router: 10.78.0.1").
6. Rules are almost invisible on the dark theme: the app menu gap between "Settings" and "Show All" reads as a blank row.
7. Empty states say nothing. With no Bluetooth adapter both Bluetooth menus are one line, "Bluetooth Settings…", with no reason.
8. App tray menus lose information: icons, check marks, radio choices and shortcuts that apps send are dropped.
9. Submenus replace the menu with a "Back" row and no title, so you lose where you are.
10. No keyboard control. Arrow keys, Enter and Escape do nothing in a menu.
11. Notification Center shows letter avatars for Drifter's own notes (Wi-Fi, Bluetooth, AirDrop) instead of their glyphs.
12. Menus open with a big scale jump (0.35 to 1) from the click point. Panels fade in at 0.96. Two motions for the same idea.
13. The power menu order is odd: Lock, Log out, Shut down, Suspend, Restart. No glyphs and no hint of the lock shortcut.

## 3. The shared language

One surface:
- Radius 14 for every panel and menu; tooltips 10.
- Backing 0.96 with the frost behind it, Control Center's value, for every surface (`veil` applies on top, as now).
- Shadow `Shadow::Menu` for all.
- Inner padding 6 px for menus, 12 px for panels.
- Rule: a hairline at 10% ink, inset 12 px, with 5 px space above and below.

One type scale:
- Title 13 px, weight 600, ink.
- Row 13 px, weight 400, ink.
- Note 11 px, dim.
- Section label 11 px, weight 600, dim, with 8 px above.

One set of rows (a new `menu::Row` kind on each item):
- Action: optional leading glyph (16 px, ink), label, and on the right a dim shortcut, a check mark or a chevron. 28 px tall.
- Toggle header: title on the left, switch on the right (the switch drawn like Settings'). 36 px tall.
- Section label.
- Rich row: 26 px disc (accent when on), name, note under it, and on the right bars, battery, lock or a check. 40 px tall. Wi-Fi and Bluetooth use these, drawn from the same art as Control Center.
- Info row: key in dim on the left, value in ink on the right. 24 px tall. Not clickable, so no hover.
- Message row: a short centred dim sentence for empty states, with an optional link under it.
- Rule.

One hover:
- Control Center's own soft highlight (`lit.rs`: radius 9, 7% ink, glides 0.12 s) on every row. The text stays ink. The menu image is painted once and not repainted on hover.
- A pressed row flashes the highlight to 16% for 0.08 s before the menu closes.

One motion:
- Open: fade in and drop 6 px from the bar, scale 0.96 to 1 on `drawer::ease`, 0.2 s. Rich rows stagger in 0.02 s apart.
- Close: fade out 0.12 s on `smooth`.
- Submenu: the new level slides in from the right by 24 px while the old level slides out to the left, 0.22 s, and the panel height eases to the new size.
- Reduce Motion: fades only.

One keyboard:
- Up and Down move the wash. Enter picks. Escape closes. Right opens a submenu, Left goes back.

---

## 4. Punch list

### Phase 1: the shared pieces

- [ ] **1. Row kinds in `menu::Item`.**
  - Add `kind: Kind { Action, Header, Section, Rich, Info, Message }` and the fields they need: `glyph: Option<Glyph>`, `note`, `trail: Option<Trail { Shortcut(String), Check(bool), Radio(bool), Bars(u8), Battery(u8), Lock, Chevron }>`, `switch: Option<bool>`, `value` (for Info).
  - `Item::line`, `more` and `rule` stay, so callers keep working.
  - Heights per kind in `Look::height` and `item_rect`.
- [ ] **2. One look for every surface.**
  - `menu::Look`: radius 14, backing 0.96, pad 6, the type scale above. The accent already follows the user's accent (follow.rs:294).
  - `calendar::Look`, the charges card and `glance::Card`: backing 0.96 and the same rule and section style. `control::Look` is the reference and does not change.
  - Tooltips keep radius 10.
- [ ] **3. Paint every row kind.**
  - New file `crates/drifter-core/src/menu/rows.rs` (split `menu.rs`, now 631 lines): paint Action, Header with switch, Section, Rich (disc and trailing marks from the control art), Info, Message, Rule.
  - Glyph set: a small SVG set for lock, log out, sleep, restart, power, hide, quit, settings, about, check, radio, chevron.
- [ ] **4. GPU hover wash for menus.**
  - Move menus onto `lit.rs` (Pane::Menu and Pane::DeskMenu join `row_under`). Stop repainting the menu on hover (`repaint_popover`, bars.rs:88).
  - Press flash before close.
- [ ] **5. One open and close motion.**
  - `menu::grown` becomes the drop: fade, 6 px drop, 0.96 to 1. Panels use the same in fade.rs `pane_look`.
- [ ] **6. Keyboard in menus.**
  - Up, Down, Enter, Escape, Right, Left while a popover is open (input.rs:699).

### Phase 2: our own menus

- [ ] **7. Wi-Fi left click.** Header "Wi-Fi" with switch. Section "Known", rich rows (disc, name, "Joined" or "Saved", lock, bars). Section "Other networks". Action "Other…" with chevron. Section "VPN" with switch rows. Rule. "Wi-Fi Settings…" with the settings glyph.
- [ ] **8. Wi-Fi right click.** Header with the network name and switch. Info rows: IP address, Router, Band, Channel, Security, Signal, Speed. Rule. "Wi-Fi Settings…". Off or not joined: a message row.
- [ ] **9. Bluetooth left click.** Header "Bluetooth" with switch. Rich rows for your devices (disc with the device glyph, name, "Connected" or "Not connected", battery). Empty: message row "No devices set up yet." with "Set Up a Device…".
- [ ] **10. Bluetooth right click.** Header with switch. One rich row per connected device with battery and sound quality as the note. Rule. "Bluetooth Settings…". No adapter or service: a message row that says why, with "Fix It" or "Turn On the Bluetooth Service".
- [ ] **11. Power menu.** Glyph rows in this order: Lock (with its shortcut), Suspend, rule, Restart, Shut Down, rule, Log Out.
- [ ] **12. App menu.** About Drifter and Settings with glyphs; Hide, Hide Others, Show All, rule, Quit with the app's own icon on the Quit row. Fix the invisible rule.

### Phase 3: other apps' menus and the panels

- [ ] **13. App tray menus keep what apps send.** Parse `icon-name`, `icon-data`, `toggle-type`, `toggle-state` and `shortcut` in tray.rs (:324-338). Draw icons, check marks, radio dots and shortcuts with the shared rows.
- [ ] **14. Submenus with a title.** A header row with a back chevron and the submenu's name, and the slide between levels.
- [ ] **15. Panels speak the same language.** Batteries and Glance: header style, section labels, rule and backing from item 2. Control Center and Notification Center: same rule and section style.
- [ ] **16. Notification Center glyphs.** Store a glyph key with Drifter's own notes (Wi-Fi, Bluetooth, AirDrop, screenshots) and draw that glyph, not a letter.

---

## Reference (not work)

- The desktop right-click menu already shares `menu::Look` (deskmenu.rs), so it picks up items 1 to 6 for free.
- Dock and Launchpad menus use the same popover and also get the new look; their item lists do not change in this round.

Coverage: read menu.rs (Item, Look, paint, grown), bars.rs (open_popover, repaint, press, tips, menu_look), appname.rs, power.rs items, tray.rs menu parse, control.rs, calendar/charges/glance looks, fade.rs pane looks, lit.rs. Screenshots from the trixie VM of: app menu, power menu, Wi-Fi left and right, Bluetooth left and right, Batteries, Control Center, Notification Center, tooltip. Glance needs live stats the VM lacks, so it was read from code.
