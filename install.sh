#!/bin/sh
# Kota — her Linux dağıtımı için kurulum betiği (sudo gerekmez).
#
#   curl -fsSL https://raw.githubusercontent.com/ensignalp/kota/main/install.sh | sh
#
# Ya da indirilen sürüm arşivinin içinden:  ./install.sh
#
# Seçenekler:
#   --uninstall        kaldır
#   --prefix DİZİN     varsayılan ~/.local
#   --version SÜRÜM    belirli bir sürüm (ör. 0.6.0); varsayılan en yenisi
#
# Kurulan: ~/.local/bin/kota (statik, kütüphane bağımlılığı yok), D-Bus
# etkinleştirme dosyası, systemd kullanıcı birimi (systemd varsa) ve GNOME
# Shell eklentisi (GNOME varsa).

set -eu

REPO="ensignalp/kota"
UUID="kota@ensignalp"
BUS="io.github.ensignalp.Kota"
PREFIX="${HOME}/.local"
VERSION=""
ACTION="install"

while [ $# -gt 0 ]; do
    case "$1" in
        --uninstall) ACTION="uninstall" ;;
        --prefix) PREFIX="$2"; shift ;;
        --version) VERSION="$2"; shift ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) echo "bilinmeyen seçenek: $1" >&2; exit 2 ;;
    esac
    shift
done

BINDIR="${PREFIX}/bin"
DATADIR="${XDG_DATA_HOME:-${HOME}/.local/share}"
EXT_DIR="${DATADIR}/gnome-shell/extensions/${UUID}"
DBUS_DIR="${DATADIR}/dbus-1/services"
UNIT_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
has_systemd() { have systemctl && systemctl --user show-environment >/dev/null 2>&1; }

if [ "$(id -u)" = 0 ]; then
    echo "Kota kullanıcı başına kurulur; sudo olmadan çalıştırın." >&2
    exit 1
fi

uninstall() {
    if has_systemd; then
        systemctl --user disable --now kota.service 2>/dev/null || true
    fi
    have gnome-extensions && gnome-extensions disable "$UUID" 2>/dev/null || true
    rm -f "${BINDIR}/kota" "${UNIT_DIR}/kota.service" "${DBUS_DIR}/${BUS}.service"
    rm -rf "$EXT_DIR"
    has_systemd && systemctl --user daemon-reload || true
    say "Kota kaldırıldı. (Ayarlar: ~/.config/kota — elle silebilirsiniz.)"
}

if [ "$ACTION" = uninstall ]; then
    uninstall
    exit 0
fi

# ---------------------------------------------------------------- kaynak
SRC=""
cleanup() { [ -n "${TMP:-}" ] && rm -rf "$TMP"; }
trap cleanup EXIT

here="$(cd "$(dirname "$0")" 2>/dev/null && pwd || echo .)"
if [ -x "${here}/kota" ] && [ -d "${here}/gnome-extension/${UUID}" ]; then
    SRC="$here"                                   # sürüm arşivinin içinden
else
    case "$(uname -m)" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "Desteklenmeyen mimari: $(uname -m). Kaynaktan kurun: make install" >&2; exit 1 ;;
    esac
    if [ -z "$VERSION" ]; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -1)
        [ -n "$VERSION" ] || { echo "En son sürüm bulunamadı (ağ?)." >&2; exit 1; }
    fi
    NAME="kota-${VERSION}-${ARCH}-linux"
    TMP=$(mktemp -d)
    say "Kota ${VERSION} (${ARCH}) indiriliyor…"
    curl -fsSL "https://github.com/${REPO}/releases/download/v${VERSION}/${NAME}.tar.gz" -o "${TMP}/k.tar.gz"
    tar -xzf "${TMP}/k.tar.gz" -C "$TMP"
    SRC="${TMP}/${NAME}"
fi

# ---------------------------------------------------------------- kurulum
say "Kuruluyor: ${BINDIR}/kota"
mkdir -p "$BINDIR" "$DBUS_DIR"
install -m755 "${SRC}/kota" "${BINDIR}/kota"
sed "s|@BINDIR@|${BINDIR}|g" "${SRC}/packaging/dbus/${BUS}.service" > "${DBUS_DIR}/${BUS}.service"
rm -f "${DBUS_DIR}/io.github.mustazafi.Kota.service"   # 0.5.0 öncesi ad

if has_systemd; then
    mkdir -p "$UNIT_DIR"
    sed "s|@BINDIR@|${BINDIR}|g" "${SRC}/packaging/systemd/kota.service" > "${UNIT_DIR}/kota.service"
    systemctl --user daemon-reload
    systemctl --user enable kota.service >/dev/null 2>&1 || true
    systemctl --user restart kota.service || true
else
    # systemd yoksa servis D-Bus etkinleştirmesiyle ilk istekte başlar.
    say "systemd bulunamadı: servis ilk kullanımda D-Bus tarafından başlatılacak."
fi

if have gnome-shell; then
    say "GNOME Shell eklentisi kuruluyor"
    rm -rf "${DATADIR}/gnome-shell/extensions/kota@mustazafi" "$EXT_DIR"
    mkdir -p "$EXT_DIR"
    cp -r "${SRC}/gnome-extension/${UUID}/." "$EXT_DIR/"
    if have glib-compile-schemas; then
        glib-compile-schemas "${EXT_DIR}/schemas"
    fi
    if ! gnome-extensions enable "$UUID" 2>/dev/null; then
        current=$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || echo "[]")
        case "$current" in
            *"'$UUID'"*) ;;
            "@as []"|"[]") gsettings set org.gnome.shell enabled-extensions "['$UUID']" ;;
            *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$UUID']" ;;
        esac
        say "Eklenti etkinleştirildi; görünmesi için oturumu kapatıp yeniden açın (Wayland)."
    fi
else
    echo "GNOME Shell yok: eklenti atlandı. CLI ve Waybar modülü kullanılabilir (kota --waybar)."
fi

# ---------------------------------------------------------------- öneriler
if ! have secret-tool; then
    hint="libsecret araçları"
    if have apt-get; then hint="sudo apt install libsecret-tools"
    elif have dnf; then hint="sudo dnf install libsecret"
    elif have pacman; then hint="sudo pacman -S libsecret"
    elif have zypper; then hint="sudo zypper install libsecret-tools"
    elif have apk; then hint="sudo apk add libsecret"
    elif have xbps-install; then hint="sudo xbps-install libsecret"
    elif have emerge; then hint="sudo emerge app-crypt/libsecret"
    fi
    echo
    echo "Not: API anahtarlarını GNOME Anahtarlığı'nda saklamak için secret-tool gerekir:"
    echo "     ${hint}"
    echo "     (Claude/Codex/Gemini/Copilot girişleri bunsuz da çalışır.)"
fi

case ":${PATH}:" in
    *":${BINDIR}:"*) ;;
    *) echo; echo "Not: ${BINDIR} PATH'te değil; kabuk yapılandırmanıza ekleyin." ;;
esac

echo
say "Kota kuruldu. Deneyin: kota"
