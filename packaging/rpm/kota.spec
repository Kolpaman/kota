# Kota — Fedora/COPR paketi.
# COPR'da "Enable internet access during builds" açık olmalı (cargo bağımlılıkları
# derleme sırasında indirilir) ya da kaynak arşivi `cargo vendor` ile hazırlanmalı.

%global uuid kota@ensignalp

Name:           kota
Version:        0.6.0
Release:        1%{?dist}
Summary:        AI usage limits in the GNOME top bar (Claude, Codex, Gemini, Copilot…)
License:        MIT
URL:            https://github.com/ensignalp/kota
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  systemd-rpm-macros
BuildRequires:  glib2
Requires:       libsecret
Recommends:     gnome-shell-extension-kota = %{version}-%{release}

%description
Kota shows how much of your AI subscription limits you have used: Claude and
Codex plan windows, Gemini daily quotas, GitHub Copilot premium requests and
API balances. A small D-Bus service (`kota daemon`) does the fetching; the
GNOME Shell extension only displays the data.

%package -n gnome-shell-extension-kota
Summary:        GNOME Shell extension for Kota
BuildArch:      noarch
Requires:       gnome-shell >= 48
Requires:       %{name} = %{version}-%{release}

%description -n gnome-shell-extension-kota
Top bar indicator, Quick Settings tile and calendar section for Kota.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release --locked

%install
install -Dm755 target/release/kota %{buildroot}%{_bindir}/kota
sed 's|@BINDIR@|%{_bindir}|g' packaging/systemd/kota.service \
    | install -Dm644 /dev/stdin %{buildroot}%{_userunitdir}/kota.service
sed 's|@BINDIR@|%{_bindir}|g' packaging/dbus/io.github.ensignalp.Kota.service \
    | install -Dm644 /dev/stdin %{buildroot}%{_datadir}/dbus-1/services/io.github.ensignalp.Kota.service
install -d %{buildroot}%{_datadir}/gnome-shell/extensions/%{uuid}
cp -r gnome-extension/%{uuid}/. %{buildroot}%{_datadir}/gnome-shell/extensions/%{uuid}/
rm -rf %{buildroot}%{_datadir}/gnome-shell/extensions/%{uuid}/schemas
install -Dm644 gnome-extension/%{uuid}/schemas/org.gnome.shell.extensions.kota.gschema.xml \
    %{buildroot}%{_datadir}/glib-2.0/schemas/org.gnome.shell.extensions.kota.gschema.xml

%check
cargo test --release --locked --workspace

%post
%systemd_user_post kota.service

%preun
%systemd_user_preun kota.service

%files
%license LICENSE
%doc README.md config.example.toml
%{_bindir}/kota
%{_userunitdir}/kota.service
%{_datadir}/dbus-1/services/io.github.ensignalp.Kota.service

%files -n gnome-shell-extension-kota
%license LICENSE
%{_datadir}/gnome-shell/extensions/%{uuid}/
%{_datadir}/glib-2.0/schemas/org.gnome.shell.extensions.kota.gschema.xml

%changelog
* Sun Nov 08 2026 Muhammed Alparslan <ensignalp@users.noreply.github.com> - 0.6.0-1
- Builds for every Linux distribution: static binary, .deb, Arch PKGBUILD,
  universal install.sh; fall back to bundled CA roots; more terminals.

* Sat Nov 07 2026 Muhammed Alparslan <ensignalp@users.noreply.github.com> - 0.5.1-1
- Rename identifiers to ensignalp: extension kota@ensignalp, D-Bus
  io.github.ensignalp.Kota. Stored keys and profiles migrate automatically.

* Sat Nov 07 2026 Muhammed Alparslan <ensignalp@users.noreply.github.com> - 0.5.0-1
- Usage history (7/30 days), per-agent tokens, account profiles, run-out
  warnings, light theme, limit-aware colors, Kota logo panel option.
