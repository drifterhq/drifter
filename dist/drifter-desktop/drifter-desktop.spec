Name:           drifter-desktop
Version:        @VERSION@
Release:        1%{?dist}
Summary:        The full Drifter desktop
License:        MIT
URL:            https://github.com/donuts-are-good/drifter
BuildArch:      noarch

Requires:       drifter >= %{version}
Requires:       drifter-explorer >= %{version}
Requires:       foot
Requires:       gnome-text-editor
Requires:       loupe
Requires:       papers
Requires:       celluloid
Requires:       gnome-calculator
Requires:       gnome-keyring
Requires:       gnome-keyring-pam
Requires:       PackageKit
Requires:       xorg-x11-server-Xwayland
Requires:       NetworkManager-wifi
Requires:       bluez
Requires:       pipewire
Requires:       wireplumber
Recommends:     drifter-greeter

%description
Drifter with the apps a desktop needs: Explorer for files, Foot for a
terminal, Text Editor, Loupe for pictures, Papers for PDFs, Celluloid for
video and music, Calculator, the GNOME keyring for passwords, and
PackageKit for software updates in Settings.

%files
