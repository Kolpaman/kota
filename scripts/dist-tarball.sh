#!/bin/sh
# Statik ikiliden dağıtıma bağımsız sürüm arşivi üretir.
#   scripts/dist-tarball.sh <kota-ikilisi> <mimari: x86_64|aarch64> <sürüm>
# Çıktı: dist/kota-<sürüm>-<mimari>-linux.tar.gz (içinde install.sh)
set -eu
bin="$1"; arch="$2"; ver="$3"
name="kota-${ver}-${arch}-linux"
out="dist/${name}"
rm -rf "$out"
mkdir -p "$out/packaging" "$out/gnome-extension"
install -m755 "$bin" "$out/kota"
cp -r packaging/dbus packaging/systemd "$out/packaging/"
cp -r gnome-extension/kota@ensignalp "$out/gnome-extension/"
rm -f "$out/gnome-extension/kota@ensignalp/schemas/gschemas.compiled"
cp install.sh README.md LICENSE config.example.toml "$out/"
tar -C dist -czf "dist/${name}.tar.gz" "$name"
rm -rf "$out"
echo "dist/${name}.tar.gz"
