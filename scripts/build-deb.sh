#!/bin/sh
# Statik ikiliden Debian/Ubuntu paketi (dpkg-deb gerekir).
#   scripts/build-deb.sh <kota-ikilisi> <mimari: x86_64|aarch64> <sürüm>
# Çıktı: dist/kota_<sürüm>_<amd64|arm64>.deb
set -eu
bin="$1"; arch="$2"; ver="$3"
case "$arch" in x86_64) darch=amd64 ;; aarch64) darch=arm64 ;; *) echo "mimari?" >&2; exit 1 ;; esac
root="$(mktemp -d)/kota_${ver}_${darch}"
uuid="kota@ensignalp"
install -Dm755 "$bin" "$root/usr/bin/kota"
sed 's|@BINDIR@|/usr/bin|g' packaging/systemd/kota.service | install -Dm644 /dev/stdin "$root/usr/lib/systemd/user/kota.service"
sed 's|@BINDIR@|/usr/bin|g' packaging/dbus/io.github.ensignalp.Kota.service \
    | install -Dm644 /dev/stdin "$root/usr/share/dbus-1/services/io.github.ensignalp.Kota.service"
mkdir -p "$root/usr/share/gnome-shell/extensions/$uuid"
cp -r "gnome-extension/$uuid/." "$root/usr/share/gnome-shell/extensions/$uuid/"
install -Dm644 "gnome-extension/$uuid/schemas/org.gnome.shell.extensions.kota.gschema.xml" \
    "$root/usr/share/glib-2.0/schemas/org.gnome.shell.extensions.kota.gschema.xml"
rm -rf "$root/usr/share/gnome-shell/extensions/$uuid/schemas"
install -Dm644 LICENSE "$root/usr/share/doc/kota/copyright"
install -Dm644 README.md "$root/usr/share/doc/kota/README.md"
size=$(du -sk "$root/usr" | cut -f1)
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<CTRL
Package: kota
Version: ${ver}
Architecture: ${darch}
Maintainer: Muhammed Alparslan <ensignalp@users.noreply.github.com>
Installed-Size: ${size}
Recommends: libsecret-tools, gnome-shell (>= 48)
Section: utils
Priority: optional
Homepage: https://github.com/ensignalp/kota
Description: AI usage limits in the GNOME top bar
 Shows Claude and Codex plan windows, Gemini daily quotas, GitHub Copilot
 premium requests and API balances. Includes the kota D-Bus service and the
 GNOME Shell extension (kota@ensignalp).
CTRL
cat > "$root/DEBIAN/postinst" <<'POST'
#!/bin/sh
set -e
if [ "$1" = configure ] && command -v glib-compile-schemas >/dev/null; then
    glib-compile-schemas /usr/share/glib-2.0/schemas || true
fi
POST
cat > "$root/DEBIAN/postrm" <<'POST'
#!/bin/sh
set -e
if command -v glib-compile-schemas >/dev/null; then
    glib-compile-schemas /usr/share/glib-2.0/schemas || true
fi
POST
chmod 755 "$root/DEBIAN/postinst" "$root/DEBIAN/postrm"
mkdir -p dist
dpkg-deb --root-owner-group --build "$root" "dist/kota_${ver}_${darch}.deb" >/dev/null
rm -rf "$(dirname "$root")"
echo "dist/kota_${ver}_${darch}.deb"
