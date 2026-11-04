# Drifter

A Wayland desktop where the screen is a camera over one large world.
Windows stay where you put them.

## Install

Drifter has signed package repos for Debian 13, Fedora 44 and Arch Linux.
The steps for each are at <https://drifterhq.github.io/drifter/>.

Install `drifter-desktop` for Drifter with the apps a desktop needs:
Explorer, Terminal, Text Editor, pictures, PDFs, video, the keyring,
printers and software updates. Install `drifter` for the desktop alone.

Then pick Drifter from your login screen's session menu. For Drifter's own
login screen, install `drifter-login` and run
`sudo systemctl enable drifter-login` after you turn off your current login
manager.

## Build from source

    scripts/zig.sh
    cargo build --release
    scripts/install.sh

`scripts/zig.sh` installs Zig 0.17.1, which builds the terminal core.
`scripts/install.sh` sets up the session, the login screen and the desktop
files so this checkout runs as your desktop.

Packages: `scripts/build-deb.sh` on Debian, `scripts/build-rpm.sh` on
Fedora, `scripts/build-arch.sh` on Arch.

## Releases

Each push to `main` builds every package. When the version in `Cargo.toml`
has no release yet, the build also signs the repos, publishes them, and
makes the GitHub release with the notes from [CHANGELOG.md](CHANGELOG.md).

## License

MIT, see [LICENSE](LICENSE). Vendored code keeps its own license: see
`vendor/*/LICENSE`.
