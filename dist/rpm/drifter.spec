%global zigver 0.17.1

Name:           drifter
Version:        0.1.0
Release:        1%{?dist}
Summary:        A Wayland desktop that is a camera over one large world
License:        MIT
URL:            https://github.com/drifterhq/drifter
Source0:        drifter-%{version}.tar.gz
Source1:        https://ziglang.org/download/%{zigver}/zig-%{_arch}-linux-%{zigver}.tar.xz
ExclusiveArch:  x86_64 aarch64

BuildRequires:  cargo >= 1.98
BuildRequires:  rust >= 1.98
BuildRequires:  clang
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  systemd-rpm-macros
BuildRequires:  pkgconfig(libseat)
BuildRequires:  pkgconfig(libinput)
BuildRequires:  pkgconfig(libudev)
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(gbm)
BuildRequires:  pkgconfig(egl)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(wayland-egl)
BuildRequires:  pkgconfig(libdrm)
BuildRequires:  pkgconfig(pixman-1)
BuildRequires:  pam-devel
BuildRequires:  xz

Requires:       adwaita-icon-theme
Requires:       mesa-dri-drivers
Requires:       mesa-libEGL
Requires:       fontconfig
Requires:       dbus
Requires:       polkit
Requires:       systemd
Requires:       pipewire
Requires:       wireplumber
Requires:       xdg-desktop-portal
Requires:       xdg-desktop-portal-wlr
Requires:       xdg-desktop-portal-gtk
Recommends:     drifter-explorer = %{version}-%{release}
Recommends:     drifter-terminal = %{version}-%{release}
Recommends:     xorg-x11-server-Xwayland
Recommends:     xwayland-satellite >= 0.8.3
Recommends:     NetworkManager-wifi
Recommends:     bluez
Recommends:     bluez-obexd
Recommends:     udisks2
Recommends:     power-profiles-daemon
Recommends:     iw
Recommends:     util-linux
Recommends:     glib2
Recommends:     grim
Recommends:     wf-recorder
Recommends:     pulseaudio-utils
Recommends:     xsettingsd
Recommends:     accountsservice
Recommends:     openssl
Recommends:     gnome-keyring
Recommends:     gnome-keyring-pam
Recommends:     gcr
Recommends:     cups
Recommends:     cups-pk-helper
Suggests:       orca

%description
A Wayland desktop where the window manager is a camera over one large world.
Windows stay where you put them.

%package explorer
Summary:        A file manager made for Drifter
Recommends:     ffmpegthumbnailer
Recommends:     poppler-utils
Recommends:     bsdtar
Recommends:     zip
Recommends:     unzip
Suggests:       7zip

%description explorer
A file manager made for Drifter: a folder tree, a path bar, list, icon,
column and gallery views, and the Trash.

%package terminal
Summary:        A terminal made for Drifter

%description terminal
A terminal made for Drifter. Ghostty's terminal core, libghostty-vt, reads
the shell's output; Drifter draws it, with tabs and themes.

%package login
Summary:        Drifter's login service
Requires:       %{name} = %{version}-%{release}
%{?systemd_requires}

%description login
Drifter's own login service. It shows the Drifter login screen and starts
each session with no black screen between them. Turn it on with:
sudo systemctl enable drifter-login

%package greeter
Summary:        Drifter's login screen for greetd
BuildArch:      noarch
Requires:       %{name} >= %{version}
Requires:       greetd

%description greeter
The Drifter lock screen as the login screen, run by greetd. Pick a person
with the arrow keys or by clicking the name, and pick a session from the
power menu in the corner.

%package desktop
Summary:        The full Drifter desktop
BuildArch:      noarch
Requires:       %{name} >= %{version}
Requires:       %{name}-explorer >= %{version}
Requires:       %{name}-terminal >= %{version}
Requires:       gnome-text-editor
Requires:       loupe
Requires:       papers
Requires:       celluloid
Requires:       gnome-calculator
Requires:       gnome-keyring
Requires:       gnome-keyring-pam
Requires:       gcr
Requires:       openssh-clients
Requires:       PackageKit
Requires:       xorg-x11-server-Xwayland
Requires:       xwayland-satellite >= 0.8.3
Requires:       cups
Requires:       cups-pk-helper
Requires:       NetworkManager-wifi
Requires:       bluez
Requires:       pipewire
Requires:       wireplumber
Recommends:     %{name}-login

%description desktop
Drifter with the apps a desktop needs: Explorer for files, Terminal,
Text Editor, Loupe for pictures, Papers for PDFs, Celluloid for
video and music, Calculator, the GNOME keyring for passwords and SSH keys,
CUPS for printers, xwayland-satellite for X11 apps, and PackageKit for
software updates in Settings.

%prep
%autosetup -n drifter-%{version}
tar -xJf %{SOURCE1}

%build
export PATH=$PWD/zig-%{_arch}-linux-%{zigver}:$PATH
cargo build --release --locked \
    -p drifter -p drifter-settings -p drifter-markup -p drifter-activity \
    -p drifter-explorer -p drifter-login -p drifter-terminal

%install
for bin in drifter drifter-settings drifter-markup drifter-activity drifter-explorer; do
    install -Dm755 target/release/$bin %{buildroot}%{_bindir}/$bin
done
install -Dm755 scripts/drifter-session %{buildroot}%{_bindir}/drifter-session
install -Dm644 dist/drifter.desktop %{buildroot}%{_datadir}/wayland-sessions/drifter.desktop
install -Dm644 dist/drifter-activity.desktop %{buildroot}%{_datadir}/applications/drifter-activity.desktop
install -Dm644 dist/pam-drifter-system-auth %{buildroot}%{_sysconfdir}/pam.d/drifter
install -Dm644 dist/drifter-session.target %{buildroot}%{_userunitdir}/drifter-session.target
install -Dm644 dist/drifter-ssh-agent.service %{buildroot}%{_userunitdir}/drifter-ssh-agent.service
install -Dm644 dist/61-drifter-pixelbook.hwdb %{buildroot}%{_udevhwdbdir}/61-drifter-pixelbook.hwdb
install -Dm644 conf/drifter-portals.conf %{buildroot}%{_datadir}/xdg-desktop-portal/drifter-portals.conf
install -Dm644 conf/xdpw.conf %{buildroot}%{_sysconfdir}/xdg/xdg-desktop-portal-wlr/drifter
install -Dm644 conf/default.toml %{buildroot}%{_datadir}/drifter/conf/default.toml
install -Dm644 img/cursor.png %{buildroot}%{_datadir}/drifter/img/cursor.png
for dir in shaders fonts img/dock img/window img/users; do
    install -d %{buildroot}%{_datadir}/drifter/$dir
    cp -r $dir/. %{buildroot}%{_datadir}/drifter/$dir/
done
install -Dm644 icons/drifter-icons/index.theme %{buildroot}%{_datadir}/icons/drifter-icons/index.theme
for kind in apps places devices mimetypes; do
    install -d %{buildroot}%{_datadir}/icons/drifter-icons/256x256/$kind
    cp -r icons/drifter-icons/256x256/$kind/. %{buildroot}%{_datadir}/icons/drifter-icons/256x256/$kind/
done

install -Dm644 dist/drifter-explorer.desktop %{buildroot}%{_datadir}/applications/drifter-explorer.desktop
install -Dm644 dist/drifter-mimeapps.list %{buildroot}%{_datadir}/applications/drifter-mimeapps.list

install -Dm755 target/release/drifter-terminal %{buildroot}%{_bindir}/drifter-terminal
install -Dm644 dist/drifter-terminal.desktop %{buildroot}%{_datadir}/applications/drifter-terminal.desktop

install -Dm755 target/release/drifter-login %{buildroot}%{_libexecdir}/drifter-login
install -Dm644 dist/drifter-login/drifter-login.service %{buildroot}%{_unitdir}/drifter-login.service
install -Dm644 dist/drifter-login/login.conf %{buildroot}%{_sysconfdir}/drifter/login.conf
install -Dm644 dist/drifter-login/pam-greeter %{buildroot}%{_sysconfdir}/pam.d/drifter-greeter
install -Dm644 dist/drifter-login/pam-login-fedora %{buildroot}%{_sysconfdir}/pam.d/drifter-login
install -Dm644 dist/drifter-login/sysusers.conf %{buildroot}%{_sysusersdir}/drifter-login.conf
install -Dm644 dist/drifter-login/tmpfiles.conf %{buildroot}%{_tmpfilesdir}/drifter-login.conf

install -Dm644 dist/drifter-greeter/greetd.conf %{buildroot}%{_unitdir}/greetd.service.d/drifter-greeter.conf
sed -e 's/@VT@/1/' -e 's/@USER@/greetd/' dist/drifter-greeter/drifter.toml.in > drifter.toml
install -Dm644 drifter.toml %{buildroot}%{_sysconfdir}/greetd/drifter.toml

%post
systemd-hwdb update >/dev/null 2>&1 || :

%post login
systemd-sysusers %{_sysusersdir}/drifter-login.conf >/dev/null 2>&1 || :
systemd-tmpfiles --create %{_tmpfilesdir}/drifter-login.conf >/dev/null 2>&1 || :
%systemd_post drifter-login.service

%preun login
%systemd_preun drifter-login.service

%postun login
%systemd_postun drifter-login.service

%post greeter
systemctl daemon-reload >/dev/null 2>&1 || :

%postun greeter
systemctl daemon-reload >/dev/null 2>&1 || :

%files
%license LICENSE
%{_bindir}/drifter
%{_bindir}/drifter-settings
%{_bindir}/drifter-markup
%{_bindir}/drifter-activity
%{_bindir}/drifter-session
%{_datadir}/wayland-sessions/drifter.desktop
%{_datadir}/applications/drifter-activity.desktop
%config(noreplace) %{_sysconfdir}/pam.d/drifter
%{_userunitdir}/drifter-session.target
%{_userunitdir}/drifter-ssh-agent.service
%{_udevhwdbdir}/61-drifter-pixelbook.hwdb
%{_datadir}/xdg-desktop-portal/drifter-portals.conf
%config(noreplace) %{_sysconfdir}/xdg/xdg-desktop-portal-wlr/drifter
%{_datadir}/drifter
%{_datadir}/icons/drifter-icons

%files explorer
%{_bindir}/drifter-explorer
%{_datadir}/applications/drifter-explorer.desktop
%{_datadir}/applications/drifter-mimeapps.list

%files terminal
%license LICENSE vendor/ghostty/LICENSE
%{_bindir}/drifter-terminal
%{_datadir}/applications/drifter-terminal.desktop

%files login
%{_libexecdir}/drifter-login
%{_unitdir}/drifter-login.service
%config(noreplace) %{_sysconfdir}/drifter/login.conf
%config(noreplace) %{_sysconfdir}/pam.d/drifter-greeter
%config(noreplace) %{_sysconfdir}/pam.d/drifter-login
%{_sysusersdir}/drifter-login.conf
%{_tmpfilesdir}/drifter-login.conf

%files greeter
%{_unitdir}/greetd.service.d/drifter-greeter.conf
%config(noreplace) %{_sysconfdir}/greetd/drifter.toml

%files desktop
