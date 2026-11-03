Name:           drifter-greeter
Version:        @VERSION@
Release:        1%{?dist}
Summary:        Drifter's login screen
License:        MIT
URL:            https://github.com/donuts-are-good/drifter
BuildArch:      noarch
Source0:        greetd.conf
Source1:        drifter.toml.in

Requires:       drifter >= %{version}
Requires:       greetd

%description
The Drifter lock screen as the login screen, run by greetd. Pick a person
with the arrow keys or by clicking the name, and pick a session from the
power menu in the corner.

%install
install -Dm644 %{SOURCE0} %{buildroot}%{_unitdir}/greetd.service.d/drifter-greeter.conf
sed -e 's/@VT@/1/' -e 's/@USER@/greetd/' %{SOURCE1} > drifter.toml
install -Dm644 drifter.toml %{buildroot}%{_sysconfdir}/greetd/drifter.toml

%post
systemctl daemon-reload >/dev/null 2>&1 || :

%postun
systemctl daemon-reload >/dev/null 2>&1 || :

%files
%{_unitdir}/greetd.service.d/drifter-greeter.conf
%config(noreplace) %{_sysconfdir}/greetd/drifter.toml
