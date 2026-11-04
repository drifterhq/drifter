# Changelog

All notable changes to Drifter go in this file. Each change goes under
Unreleased first. A release moves those lines under its version number.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Version numbers follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Settings > Terminal: colors (Auto, Dark, Light), text size, cursor shape,
  background opacity, space around the text, tab bar edge, and always show
  the tab bar. Open terminals take each change at once.
- Terminal color themes. Settings > Terminal picks a dark theme and a light
  theme. Colors picks which one shows; Auto uses the light theme by day and
  the dark theme by night. A theme sets the text, background, cursor and all
  16 terminal colors. With Colors on Dark or Light, the theme row not in use
  is faded and cannot be clicked.
- 43 built-in themes: Drifter, Detroit Future, Solarized, Selenized, Tango, rxvt, XTerm,
  Catppuccin (Latte, Frappé, Macchiato, Mocha), Nord, Gruvbox, Tokyo Night
  (Night, Storm, Day), One, GitHub, Dracula and Alucard, Rosé Pine (Main,
  Moon, Dawn), Everforest, Kanagawa (Wave, Lotus), Ayu (Dark, Mirage,
  Light) and Tomorrow. Own themes load from
  `~/.config/drifter/terminal-themes/*.toml`.
- Settings > Terminal > Font picks any installed monospace font. JetBrains
  Mono stays built in and heads the list.
- Terminal: Settings at the bottom of the right-click menu opens
  Settings > Terminal.
- Terminal: Control +, Control - and Control 0 change the text size of one
  window.

### Changed

- Terminal: the right-click menu keeps only New Tab, New Window, Close Tab,
  Copy, Paste and Settings.
- Terminal: options now live in the `[terminal]` table of the Drifter config.
  `~/.config/drifter/terminal.conf` is no longer read.
- Terminal: the cursor and the active tab use your accent color.
- Settings opens one window. A second launch shows the asked page in the
  open window and brings it to the front.
- Search: an app whose keywords hold the search word ranks above a Settings
  page with that name.

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
