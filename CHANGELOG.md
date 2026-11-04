# Changelog

All notable changes to Drifter go in this file. Each change goes under
Unreleased first. A release moves those lines under its version number.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Version numbers follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Settings > Terminal > Window: Columns and Rows set the size a new terminal
  window opens at. The default is 100 by 30.
- Clipboard tools: data-control (wlr and ext) lets clipboard managers such as
  cliphist, clipman and `wl-paste --watch` read and set the clipboard and the
  primary selection. Sandboxed apps cannot use it.
- Screen capture: ext-image-copy-capture shares a whole screen, one window,
  or the pointer alone, into shared-memory or GPU buffers. New screen-share
  portals use it. The lock screen is never captured.
- Screen power: output-power-management lets tools such as wlopm turn one
  screen off and on. Moving the mouse does not wake a screen a tool turned
  off.
- Gamma tables: gamma-control lets tools such as gammastep and wlsunset set
  a screen's colors. Drifter's night light and color profile step aside while
  a tool holds the screen and come back when it lets go.
- Frame pacing: fifo and commit-timing let Vulkan and Mesa games pace frames
  to the screen.
- VR headsets: drm-lease lends a headset to VR apps such as Monado and
  SteamVR. Drifter no longer shows a headset as a desktop monitor.
- Fades: alpha-modifier lets video players fade their own surfaces.
- Settings > Displays > Adaptive sync has a third choice, Games and video. It
  turns adaptive sync on only while a full-screen app says it is a game or a
  video (content-type).
- Terminal tabs: drag a tab out of the window to give it its own window. The
  shell keeps running. Shift+click picks a run of tabs and Ctrl+click picks
  one more; drag the picked tabs out to move them together, or drop them in
  the tab bar to reorder. Right-click a tab, or double-click it, to rename
  it. Tab and window titles follow the running program.
- Terminal: drag to select text; double-click picks a word and triple-click
  a line. Copy and Paste work from the right-click menu, Ctrl+Shift+C and
  Ctrl+Shift+V, and the middle button pastes. The wheel scrolls back through
  old output, and Shift+Page Up and Page Down scroll by a page. Full-screen
  programs get the wheel as arrow keys or mouse reports.
- Terminal: New Window opens in the same program, so tabs can move between
  its windows.
- Settings > Terminal: colors (Auto, Dark, Light), text size, cursor shape,
  background opacity, space around the text, tab bar edge, and always show
  the tab bar. Open terminals take each change at once.
- Terminal color themes. Settings > Terminal picks a dark theme and a light
  theme. Colors picks which one shows; Auto uses the light theme by day and
  the dark theme by night. A theme sets the text, background, cursor and all
  16 terminal colors. With Colors on Dark or Light, the theme row not in use
  is faded and cannot be clicked.
- 48 built-in themes: Drifter, Detroit Future, Solarized, Selenized, Tango,
  rxvt, XTerm, Catppuccin (Latte, Frappé, Macchiato, Mocha), Nord, Gruvbox
  (and Hard), Tokyo Night (Night, Storm, Moon, Day), One, GitHub (and Dark
  Dimmed), Dracula and Alucard, Rosé Pine (Main, Moon, Dawn), Everforest,
  Kanagawa (Wave, Dragon, Lotus), Ayu (Dark, Mirage, Light) and Tomorrow.
  Their colors come from iTerm2-Color-Schemes (MIT). Own themes load from
  `~/.config/drifter/terminal-themes/*.toml`.
- Terminal: a change of theme or of Colors fades over 1 second. Reduce motion
  makes it instant. The cursor takes the theme's cursor color.
- Settings > Terminal > Font picks any installed monospace font. JetBrains
  Mono stays built in and heads the list.
- Terminal: Settings at the bottom of the right-click menu opens
  Settings > Terminal.
- Terminal: Control +, Control - and Control 0 change the text size of one
  window.
- Terminal: a double click on empty tab bar space opens a new tab.
- Settings > Terminal: Dark theme and Light theme open a grid of swatches.
  Each card shows the theme background with its text color, blue, magenta
  and green.
  Drifter Dark or Drifter Light comes first, then A to Z.

### Changed

- The hover glide now also runs in the desktop right-click menu, the
  screenshot bar, the Welcome window, Explorer (menu bar, toolbar, view
  switcher, path, sidebar, files, icons, columns, strip and tabs), Activity
  Monitor (sidebar, process rows, column heads, buttons, Hour segments) and
  Markup (tools, weights, Crop, Undo, Redo, Copy, Trash). Their old static
  hover fills are gone.
- Settings: the hover glide from menus and terminal tabs now runs on the
  sidebar sections, rows that open a page, segment options, buttons, dialog
  rows, theme cards and pictures. A click flashes it brighter. Reduce motion
  makes it jump with no slide.

- Terminal: the right-click menu keeps only New Tab, New Window, Close Tab,
  Copy, Paste and Settings.
- Terminal: options now live in the `[terminal]` table of the Drifter config.
  `~/.config/drifter/terminal.conf` is no longer read.
- Terminal: the cursor and the active tab use your accent color.
- Settings: the sidebar scrolls to show the open page, also when another app
  opens Settings on a page.
- Settings opens one window. A second launch shows the asked page in the
  open window and brings it to the front.
- Search: an app whose keywords hold the search word ranks above a Settings
  page with that name.
- Smooth scrolling is off by default. A mouse wheel moves the page at once.
  A touchpad already moves the page with your fingers.

### Fixed

- Settings no longer closes when you scroll on the About page. The live logo
  now asks for one redraw at a time. Before, each scroll added more redraws
  until the desktop dropped the window.

## [0.1.0] - 2026-11-04

The first release.

### Added

- The desktop: a Wayland compositor where the screen is a camera over one
  large world. Windows stay where you put them.
- Dock, top bar, menus, control center, tray, notifications, Spotlight
  search and a launcher whose app list updates live.
- Lock screen, polkit agent, keyring unlock at login, and an SSH agent for
  each session.
- Drifter Login: Drifter's own login service, with no black screen between
  the password and the desktop. A greetd login screen is there too.
- Settings: displays and color profiles, appearance, wallpapers and live
  wallpapers, Wi-Fi, Bluetooth, sound, power, users, accessibility,
  languages, printers and software updates.
- Explorer, a file manager with tree, list, icon, column and gallery views
  and the Trash.
- Terminal, on Ghostty's terminal core (libghostty-vt), with tabs and themes.
  It is the default terminal.
- Activity, a process and resource monitor, and Markup, for screenshots.
- AirDrop with Apple devices through owly.
- English, Spanish, German and French.
- Packages for Debian 13, Fedora 44 and Arch Linux, built by GitHub Actions
  and published in signed apt, dnf and pacman repos.
- drifter-desktop, a package that installs Drifter with the apps a desktop
  needs.
