# Changelog

All notable changes to Drifter go in this file. Each change goes under
Unreleased first. A release moves those lines under its version number.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Version numbers follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
