# Network punch list: our own Wi-Fi icon, menu, list and agent

Goal: the network icon gets the same care as Bluetooth (`txt/bluetooth.md` items, main 5e4728d). Drifter talks to NetworkManager itself, draws its own icon, and nm-applet never starts.

Every claim was checked against the code at main `64a73d9` (2026-10-22). Line numbers drift: re-check them before each edit.

Main finding: most of the work is reuse. The new parts are a NetworkManager service (`drifter-net`), a secret agent and Wi-Fi rows. The motion, panel, prompt, pill and tray-morph code already exist in `render/bluetooth.rs`.

Build order: 1 to 5, then 6 to 11, then 12 to 17, then 18 to 22, then 23 to 26.

Code that changes (summary):
- New crate `drifter-net`, built like `drifter-blue/bus.rs`: zbus, `watch()` on PropertiesChanged, a snapshot into a core `Net` struct.
- Core `net.rs` gets the D-Bus model. The nmcli parsers stay only until Settings moves over, then they go.
- `List` and `Row` move out of `Radio` into a shared Control Center list with an owner (Wi-Fi or Bluetooth). Only one list is open at a time.
- `control::What` gets a `Network` row: name, bars, lock, joined, note.
- `Hit::More` also works for the Wi-Fi tile.
- We draw `tray.net` when NetworkManager runs the card. On iwd or ConnMan, the third-party icon slot stays.

Motion spec (exact Bluetooth numbers, all divided by `window_pace`):
- List opens in 0.38 s and closes in 0.2 s. The panel uses its three-piece height tween.
- Rows arrive with a 0.36 s spring, rising 10 px, 0.03 s apart.
- Rows move with a 0.44 s glide. A network that becomes joined jumps to the top with the bounce.
- Rows leave in 0.22 s: smooth fade plus a vertical squash.
- Signal bars, in rows and in the tray arcs, ease over 0.36 s (smooth).
- Tray state change: old art fades out, new art scales 0.6 to 1 on the drawer spring in 0.32 s.
- Tray connecting wave: each arc 30% to 100% alpha, 0.15 s apart, 1.2 s loop, only while NetworkManager says "activating".
- Badges pop 0.55 to 1 in 0.42 s and fade out in 0.2 s.
- Shake: 0.5 s, 9 px, 3 cycles, falling off as (1 − t)².
- Joined disc pops 0.55 to 1 in 0.42 s.
- Connecting ring: 24 frames, one turn per 0.9 s.
- Scan heading breathes: alpha 0.62 to 1, 1.6 s.
- Password prompt opens in 0.34 s, scale 0.96 to 1, over a 0.28 dim.
- Pill: in 0.4 s from scale 0.86, shown 2.6 s, out 0.2 s.
- Wi-Fi to Bluetooth list switch: old rows leave, new rows stagger in, the panel height retargets.
- Reduce Motion: fades only. No scale, rise, shake, pop, ring turn or wave. The icon shows its end state.

Checking: no unit tests and no render tests. Each item is checked by using it live on 114 and on the trixie VM rig (join, wrong password, hidden, PEAP).

Work rules:
- Work in number order. Each item says what it waits on. One commit per item. Branch `network`.
- Build on 114 (ssh fedora). Check each item live there and on the hostapd VM rig (join, wrong password, hidden, PEAP).
- We do not write unit tests or render tests.
- `render.rs` is 1848 lines and `render/bluetooth.rs` is 1706. Do not grow them. New compositor code goes in `render/wifi.rs` and `render/ctllist.rs`.
- No comments or documentation in our code, `///` too. Vendored code is exempt. Check `git grep -n '^\s*//'` before each commit.
- Every new `lang::t("...")` string needs a line in `crates/drifter-core/lang/{es,de,fr}.txt`, or `every_string_in_the_code_has_a_translation` (lang.rs:158) fails.
- Motion: arriving uses `drawer::ease` (spring, drawer.rs:81). Leaving and levels use `motion::smooth` (motion.rs). All times are divided by `window_pace`. `window_still` (Reduce Motion) keeps alpha and drops scale, rise, shake and wave. Pace 0 means instant.
- Any new animation adds one `|| self.wifi_moving()` term to `animating()` (render.rs:764, next to `blue_moving()` at :794).

How it works today (the items change this):
- The top-bar icon is nm-applet's tray icon. `NETWORK` (drifter/src/tray.rs:23) marks it, `net_slot()` (render/bars.rs:48) finds it, and `tray.net_rect()` (drifter-core/src/tray.rs:50) places it between the battery and Bluetooth.
- nm-applet is also NetworkManager's secret agent. Without nm-applet and without our own agent, NetworkManager cannot ask for a password. A saved network with a changed password and 802.1X networks fail with no prompt.
- The Control Center Wi-Fi tile runs `radios()` (render/centers.rs:407): two nmcli calls each time the panel opens (`refresh_radios`, centers.rs:55). The tile toggles with `nmcli radio wifi` (centers.rs:291-298). It has no list: `Look::hit` (control.rs:375-381) gives `Hit::More` only for Bluetooth, and centers.rs:184 always opens the Bluetooth list.
- Settings runs nmcli in `drifter-settings/src/net.rs` (read_wifi :26, details :58, run :155, join_hidden :212). The page is `wifi_groups` (pages.rs:733). Sheets `For::WifiHidden` and `For::WifiEnterprise` exist (sheet.rs).
- AirDrop can take the Wi-Fi card: `airdrop_holds_wifi()` (render/airdrop.rs:513).

---

## Phase 1: Base

- [ ] **1. New crate `drifter-net`: NetworkManager over D-Bus.**
  - Waits on: nothing.
  - Copy the shape of `drifter-blue/src/bus.rs`: `Bus::system()` (:100), `snapshot()` (:106), `watch(tell)` (:139) with a PropertiesChanged match on `path_namespace='/org/freedesktop/NetworkManager'`, `running()` (:123). zbus 5.19 blocking, as drifter-blue uses.
  - New files: `crates/drifter-net/Cargo.toml`, `src/lib.rs`, `src/bus.rs`. Add to the workspace `Cargo.toml`.
  - Read: `WirelessEnabled`, `WirelessHardwareEnabled`, `State`, `Connectivity`, `PrimaryConnection`, `ActiveConnections`, `Devices`. Per wireless device: `ActiveAccessPoint`, `AccessPoints`, `LastScan`, `Managed`, `State`, `StateReason`. Per AP: `Ssid` (bytes), `Strength`, `Frequency`, `Flags`, `WpaFlags`, `RsnFlags`, `HwAddress`. Saved connections from `Settings.ListConnections` + `GetSettings` (id, ssid, type, autoconnect, priority, metered).
  - Core model in `drifter-core/src/net.rs`: `pub struct Net { on, hardware, managed, state, connectivity, primary: Option<Link>, nets: Vec<Network>, wired: Option<Wired>, vpns: Vec<Vpn>, searching }`. Merge APs by SSID, keep the strongest, as `wifi_nets` (net.rs:41) does now. Joined first, then known, then by signal.
  - `Missing` enum (like `blue::Missing`, blue.rs:314): no NetworkManager, card run by another tool, no card.

- [ ] **2. `drifter-net` actions.**
  - Waits on: 1.
  - `power(on)` (WirelessEnabled), `scan()` (Device.Wireless.RequestScan, ignore the rate-limit error), `join(ssid)` (ActivateConnection when saved, else AddAndActivateConnection with no secrets, so the agent asks), `leave(ssid)` (DeactivateConnection), `forget(ssid)` (Connection.Delete), `set(ssid, autoconnect, priority, metered, mac)` (Connection.Update).
  - Plain error words: new `net::plain_error(reason: u32)` beside `blue::plain_error` (blue.rs:455). Cover NM device state reasons 7 (no secrets / wrong password), 8, 9, 10 (supplicant), 15 (DHCP), 53 (SSID not found), 36 (removed). Anything else: "Could not join {name}."

- [ ] **3. Our own NetworkManager secret agent.**
  - Waits on: 1.
  - New file `crates/drifter-net/src/agent.rs`, built like `drifter_blue::agent::serve` (agent.rs). Serve `org.freedesktop.NetworkManager.SecretAgent` at `/org/freedesktop/NetworkManager/SecretAgent` on the system bus. Register with `AgentManager.RegisterWithCapabilities` (identifier `org.drifter.network`, capability VPN_HINTS = 0x1).
  - `GetSecrets` sends `Event::Ask { ssid, kind: Psk | Enterprise | Vpn, reason, reply }` to the compositor through the chores channel, the way `blue_agent_up` does (render/bluetooth.rs:192). `CancelGetSecrets` closes the prompt. `SaveSecrets` and `DeleteSecrets` return at once (NetworkManager stores the secret system-wide).
  - Re-register when NetworkManager restarts (watch NameOwnerChanged).

- [ ] **4. nm-applet stops starting.**
  - Waits on: 3, and 7 (our icon must exist first).
  - Add `"nm-applet"` and `"nm-tray"` to `BUILT_IN` (drifter-core/src/autostart.rs:34). The array size goes from 11 to 13.
  - Only while `drifter-net` sees NetworkManager and our agent has registered (item 3). If the agent fails, nm-applet still starts, so password asks never go unanswered. On iwd or ConnMan systems, `NETWORK` (tray.rs:23) and `net_slot()` keep today's behavior for `iwgtk`, `connman-gtk` and `cmst`.

- [ ] **5. Settings Wi-Fi uses `drifter-net`.**
  - Waits on: 2.
  - Replace `read_wifi`, `rescan` and the `Job` arms in `drifter-settings/src/net.rs` with `drifter_net::Bus` calls. Keep `details` (:58), which needs `iw` for dBm and the link.
  - Remove the nmcli parsers in core `net.rs` (`terse` :18, `wifi_nets` :41, `wifi_known` :77, `wifi_managed` :86) once nothing calls them, with the tests that use them.
  - Join with a password from the Settings sheet: AddAndActivateConnection with `802-11-wireless-security.psk` set. This drops the PEAP `nmcli connection edit` stdin trick (net.rs:195).

## Phase 2: The top-bar icon

- [ ] **6. Wi-Fi glyphs.**
  - Waits on: nothing.
  - New `pub fn wifi_rune() -> &'static str` (SVG: three arcs over a dot, same stroke weight as `blue::rune()`, blue.rs:231) and `pub fn wired_rune()` (two-port plug) in core `net.rs`. Draw with `blue::svg_glyph` (blue.rs:239) at `paint::dense(20.0)`.
  - Bake as separate layers, so arcs can fade one at a time: `dot`, `arc[0..3]`, plus badges `alert` ("!"), `lock`. Ink is `self.tray.ink`. Accent is `self.look.accent`.
  - Also a 64 px glyph for notes and the pill, as `blue::glyph` (blue.rs:235).

- [ ] **7. Draw our icon in the net slot.**
  - Waits on: 1, 6.
  - New file `render/wifi.rs` with `struct Wifi` on State (bus, now, missing, art, tray_state, busy, failed, shakes, pops), like `Radio` (render/bluetooth.rs:87).
  - `tray.net` (bars.rs:45) becomes true when `drifter-net` runs or when a third-party applet holds the slot. `tray_icon_rect` (bars.rs:598) and `tray_icon_at` (bars.rs:608) check our icon first.
  - `wifi_tray_elements(elements)` pushes the layers in this table. Base arcs are 30% ink. Lit arcs are 100% ink. `signal` 0 to 100 maps to 0 to 3 lit arcs (1 at 1 to 30, 2 at 31 to 60, 3 above).

    | State | Dot | Arcs | Badge |
    |---|---|---|---|
    | Off | 45% ink | outline, 45% ink | none |
    | On, not joined | 30% | 30% | none |
    | Searching or connecting | 100% | wave (item 9) | none |
    | Joined | 100% | lit by signal | none |
    | Joined, no internet | 100% | lit by signal | `alert`, ink |
    | Sign-in needed (portal) | 100% | lit by signal | `alert`, accent |
    | Wired is the main link | `wired_rune`, 100% | none | none |
    | VPN on | as above | as above | `lock`, ink, bottom right |
    | AirDrop holds the card | as Off | as Off | none |


- [ ] **8. State changes morph.**
  - Waits on: 7.
  - Same as `blue_tray_elements` (render/bluetooth.rs:1541): on a change of row in the table, the old art fades out with `smooth` while the new art fades in and scales from `MORPH_FROM` 0.6 to 1 on `drawer::ease` over `MORPH` 0.32 s.
  - Signal changes inside "Joined" do not morph. Each arc eases its own alpha from old to new with `hud::eased_level` over `LEVEL` 0.36 s.
  - Badges pop in: scale 0.55 to 1 on `drawer::ease` over 0.42 s (`POP`, `POP_FROM`). They fade out over 0.2 s.
  - Reduce Motion: cross-fade only, no scale. Pace 0: jump.

- [ ] **9. The connecting wave.**
  - Waits on: 7.
  - While NetworkManager reports a device state from 40 (prepare) to 90 (secondaries), each arc goes 30% to 100% to 30% alpha on a cosine. Inner arc first, then 0.15 s later each next arc. One loop is 1.2 s.
  - The wave stops when the state leaves that range. It never runs while idle, so the top bar does not redraw all the time. `wifi_moving()` returns true only while the wave runs or a morph or ease is running.
  - Reduce Motion: no wave. Show all arcs at 60%.

- [ ] **10. Click opens our menu.**
  - Waits on: 7, 2.
  - Copy `open_blue_menu`, `blue_menu_pick` and `press_blue_tray` (render/bluetooth.rs:1567-1608). Add `Target::Wifi`.
  - Core `net::menu(net) -> Vec<menu::Item>` like `blue::menu` (blue.rs:386). Known networks in range first, as "Home, joined" or "Home". Then up to 6 other networks. A rule. "Turn Wi-Fi Off" or "Turn Wi-Fi On". "Wi-Fi Settings…".
  - Menu items have only a label (menu.rs:27). Signal goes in the label only as words: "weak" below 30.
  - AirDrop holds the card: the menu shows a disabled line "AirDrop is using Wi-Fi." and "Stop AirDrop".
  - Picking a network calls `wifi_join` (item 20).
  - Opening the menu starts a scan (item 17).

- [ ] **11. Alt+click shows details.**
  - Waits on: 10.
  - Alt held on click: a menu of disabled lines from `net::details` (net.rs:223): IP, router, band, channel, signal in dBm, security. Then "Wi-Fi Settings…".
  - The dBm and link lines need `iw`. Run it on a worker with `self.background`, then open the menu.

## Phase 3: The Control Center list

- [ ] **12. Share the list between Wi-Fi and Bluetooth.**
  - Waits on: nothing.
  - Move `Row` and `List` (render/bluetooth.rs:41-62), `row_y` (:1633), `eased` (:1618) and the row part of `control_layers` (:968-1088) into new `render/ctllist.rs`. Add `owner: Owner { Wifi, Bluetooth }` to `List`. Move `radio.list` to `State.control_list`.
  - `blue_toggle_list` (:622), `blue_list_refresh` (:676) and `blue_list_wind` (:754) become `list_toggle(owner)`, `list_refresh()` and `list_wind()`. Each takes the items from the owner.
  - Bluetooth must look and move the same after the move. bluetooth.rs shrinks.

- [ ] **13. Network rows.**
  - Waits on: 1, 12.
  - core `control.rs`: add `What::Network { name, note, joined, secure, bars, busy }` beside `What::Device` (:110), height 46 like Device (:130). `Item::ssid()` from key prefix `net:`.
  - New `pub fn wifi_items(seen: Result<&Net, Missing>, busy, failed) -> Vec<Item>`: heading "Known" with known networks in range, heading "Other networks" with up to 8 by signal, then link "Other…" and link "Wi-Fi Settings…".
  - Wi-Fi off: heading "Wi-Fi is off." Missing: its words plus a link, as Bluetooth does (control.rs:180-195).
  - AirDrop holds the card: heading "AirDrop is using Wi-Fi." and link "Stop AirDrop".
  - Notes: "Joined", "Connecting…", "Wrong password", "No internet", "Weak signal", "Saved", "Did not join". The rule matches `items` (control.rs:154-165).
  - Row look: 32 px disc with `wifi_rune` (accent disc when joined, as `discs` (bluetooth.rs:1066)), name, note, the existing 4-step bar art (`bars_rect`, `bar_source`) and a `lock` glyph left of the bars when secure.

- [ ] **14. The Wi-Fi tile opens the list.**
  - Waits on: 13.
  - `Look::hit` (control.rs:378): `(Tile::Wifi, false) => Hit::More(tile)` too. centers.rs:184: `ControlHit::More(tile) => self.list_toggle(owner_of(tile))`.
  - The tile dot still toggles the radio. `flip_tile` Wi-Fi arm (centers.rs:291) calls `drifter_net::Bus::power` on a worker, not `nmcli`.
  - `refresh_radios` and `radios()` (centers.rs:55, :407) go. `control_status.wifi` comes from the live `Net`.

- [ ] **15. List motion.**
  - Waits on: 14.
  - All from the Bluetooth constants (render/bluetooth.rs:8-39), now in ctllist.rs:
    - The list opens in `LIST_OPEN` 0.38 s and closes in `LIST_CLOSE` 0.2 s. The panel uses its three-piece height tween (`control_retarget`, centers.rs:91; cap, one-pixel stretch, body at bluetooth.rs:1112-1131).
    - Rows arrive over `ROW_IN` 0.36 s: alpha on `smooth`, rise `ROW_RISE` 10 px on `drawer::ease`, `ROW_STAGGER` 0.03 s apart.
    - Rows move over `ROW_MOVE` 0.44 s with `dock::glide` (dock.rs:486). A network that becomes joined jumps to the top with `bounce` true.
    - Rows leave over `ROW_OUT` 0.22 s: alpha and a vertical squash on `smooth`.
    - Bars ease over `LEVEL` 0.36 s (`row.signal`).
    - Switching owner: old rows leave, new rows stagger in, and the panel height retargets in the same frame.
  - Reduce Motion: alpha only. Pace 0: no ghosts.

- [ ] **16. Busy, joined and failed rows.**
  - Waits on: 15.
  - Busy: the `ring` (24 frames, `TURN` 0.9 s per turn, bluetooth.rs:1050-1062) spins around the disc.
  - Joined: the disc swaps to the accent disc and pops from 0.55 to 1 over 0.42 s.
  - Failed: the row shakes, `SHAKE` 0.5 s, `SHAKE_REACH` 9 px, 3 cycles, falling off as (1 − t)² (bluetooth.rs:1003-1010). The note says the `plain_error` words for `FAILED_FOR` 4 s.
  - Reduce Motion: no ring turn, no pop, no shake. The note still changes.

- [ ] **17. Scan only while someone looks.**
  - Waits on: 2, 14.
  - `wifi_search_as_needed()` like `blue_search_as_needed` (bluetooth.rs:569): scan when the list or the menu opens, then every 10 s while open.
  - While a scan runs (`LastScan` not yet newer than the request), the "Other networks" heading breathes: alpha 0.62 to 1 on a cosine, `BREATH` 1.6 s (bluetooth.rs:997-1000).

## Phase 4: Joining and problems

- [ ] **18. The password prompt.**
  - Waits on: 3.
  - The polkit queue (`prompts: Vec<Pending>`, render.rs:706; `Pending`, render.rs:353) holds only polkit requests. Make `Pending.request` an enum `Asker { Polkit(polkit::Request), Wifi(drifter_net::agent::Ask) }`. `prompt_submit` (overlays.rs:224) and `prompt_cancel` (:215) reply on the right channel.
  - The look is `prompt::Look` (prompt.rs). Message: "Enter the password for “{name}”." `user` is empty, so no account line shows.
  - Opens like pairing: `PAIR_IN` 0.34 s, scale `PAIR_FROM` 0.96 to 1 on `drawer::ease`, with the 0.28 dim (`worker::dim`, overlays.rs:316).
  - Wrong password: NetworkManager asks again with a "new secrets" flag. The prompt shakes the same way as the lock screen and shows "Wrong password. Try again." If the list is open, the network's row shakes too (item 16).
  - Enterprise and VPN asks do not use this prompt. They open Settings (item 19).

- [ ] **19. Hidden and enterprise go to Settings.**
  - Waits on: 5.
  - "Other…" in the list and menu runs `open_settings_with(&["wifi", "hidden"])` (follow.rs:45). An enterprise network runs `open_settings_with(&["wifi", "enterprise", ssid])`.
  - drifter-settings window.rs:413 already reads `args().nth(2)` for the default-apps picker. Add the same for `hidden` (`For::WifiHidden`) and `enterprise` (`For::WifiEnterprise(at)`).

- [ ] **20. Join flow and the pill.**
  - Waits on: 2, 16, 18.
  - `wifi_join(ssid)` in render/wifi.rs: mark busy, call `join` on a worker, then on the result clear busy and pop or shake.
  - The pill: generalize `blue::PillLook::paint` (used at bluetooth.rs:1198-1202) to take a glyph image, not a `Kind`. Show "Joined {name}" for `PILL_FOR` 2.6 s. It enters over `PILL_IN` 0.4 s from scale 0.86 and leaves over `PILL_OUT` 0.2 s.
  - Show the pill only when the user picked the network. Never on auto-join at login.

- [ ] **21. Captive portal note.**
  - Waits on: 1.
  - When `Connectivity` becomes 2 (portal), post a note with `show_note` (see `blue_low`, bluetooth.rs:412-436): app "Wi-Fi", summary "Sign in to {name}", the 64 px glyph, one action "Open". The action opens `http://nmcheck.gnome.org/` in the default browser, so the portal takes over.
  - Fedora checks connectivity by default. Debian needs `network-manager-config-connectivity-debian`. Add it to the `recommends` list (crates/drifter/Cargo.toml:51).

- [ ] **22. Fix It for Wi-Fi in Settings.**
  - Waits on: 5.
  - Copy the Bluetooth Fix It group (pages.rs:852-870, `blue::Step` at blue.rs:922). Steps: radio blocked in hardware (`WirelessHardwareEnabled` false), NetworkManager not running (start it with pkexec systemctl), card run by another tool (`Managed` false), Wi-Fi power save on (set `wifi.powersave = 2` on the connection), no address (reconnect), no DNS (show the plain words).
  - Show "Not working?" with a Fix It button on the Wi-Fi page, as `Id::BlueFix` (pages.rs:1017) does.

## Phase 5: Smaller wins

- [ ] **23. Ethernet.**
  - Waits on: 13.
  - When a wired device is connected, the list shows a heading-free row "Ethernet, Connected" at the top. The tray shows `wired_rune` when the primary connection is wired.
  - Cable out: the note says "Cable unplugged" while the device exists.

- [ ] **24. WireGuard rows.**
  - Waits on: 13.
  - Saved connections of type `wireguard` show under a "VPN" heading. A click toggles them (ActivateConnection or DeactivateConnection). No password asks: WireGuard keys sit in the connection.
  - Explorer: a `.conf` file gets "Add as VPN", which calls `Settings.AddConnection` with the parsed keys. Add the parser in core `net.rs`.

- [ ] **25. Per-network choices in Settings.**
  - Waits on: 5.
  - The network details sheet (`For::NetInfo`) gets: Auto-join (switch), Private address (segments Off, Fixed, Changes; maps to `cloned-mac-address` permanent, stable, random), Low data mode (switch, `connection.metered` 1 or 2).
  - Priority: "Move up" and "Move down" buttons on known networks change `connection.autoconnect-priority`.

- [ ] **26. Spotlight and a shortcut.**
  - Waits on: 20.
  - Spotlight: add `Kind::Network` beside `Kind::Device` (search.rs:62, :131, :348). Typing "wifi home" or a network name lists in-range networks. Enter joins.
  - Shortcut: a keys entry that opens the Wi-Fi menu, as Bluetooth's shortcut works through `blue_key` (bluetooth.rs:269).

---

## Reference (not work)

- Not now:
  - QR code for the password. Needs a new crate (`qrcodegen`).
  - Hotspot from this computer. It fights AirDrop for the card.
  - OpenVPN and other VPN plugins. Their password dialogs are GTK plugin programs.
  - Airplane mode. It is only Wi-Fi off plus Bluetooth off. It can join later as a tile.
- No `show password` in the prompt: `prompt::Ask` carries only a typed count (prompt.rs:7-13).
- Network rows stay text in the menu: `menu::Item` has no icon or right-side text (menu.rs:27-36).

Coverage and method: read render/bluetooth.rs (Row, List, Radio, toggle, refresh, wind, moving, control_layers, tray, menu, pill), centers.rs (tile, radios), core control.rs (Tile, Status, Hit, What, items, hit), motion.rs, tween.rs, drawer::ease, dock::glide, hud::eased_level, prompt.rs, polkit Pending and paint_prompt, tray.rs net_rect, bars.rs net slot, autostart BUILT_IN, drifter-blue bus.rs and agent.rs outlines, drifter-settings net.rs, pages.rs Wi-Fi and Fix It groups, sheet.rs For. Grep-mapped: nmcli and nm-applet users, Fix It, Kind::Device, open_settings_with, lang test.
