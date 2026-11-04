# xwayland-satellite 0.8.3 for Fedora releases that ship an older one. 0.8.2
# gives X focus to override-redirect menus, so Steam and Chromium menus close
# at once. The release sorts below Fedora's own 0.8.3, so theirs wins once it
# lands.
Name:           xwayland-satellite
Version:        0.8.3
Release:        0.drifter1%{?dist}
Summary:        Rootless Xwayland for Wayland desktops
License:        MPL-2.0
URL:            https://github.com/Supreeeme/xwayland-satellite
Source0:        %{url}/archive/v%{version}/xwayland-satellite-%{version}.tar.gz

BuildRequires:  cargo >= 1.85
BuildRequires:  rust >= 1.85
BuildRequires:  clang
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  pkgconfig(xcb)
BuildRequires:  pkgconfig(xcb-cursor)
Requires:       xorg-x11-server-Xwayland

%description
xwayland-satellite runs X11 apps in Wayland desktops that have no Xwayland
of their own, such as Drifter.

%prep
%autosetup -n xwayland-satellite-%{version}

%build
# Keep its build script from reading the git repo the build folder sits in.
export GIT_CEILING_DIRECTORIES=%{_builddir}
export VERGEN_GIT_DESCRIBE=v%{version}
cargo build --release --locked

%install
install -Dm755 target/release/xwayland-satellite %{buildroot}%{_bindir}/xwayland-satellite

%files
%license LICENSE
%doc README.md
%{_bindir}/xwayland-satellite
