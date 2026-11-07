#!/bin/sh
# GNOME eklentisini etkinleştirir. Wayland'da yeni kurulan bir eklenti, Shell
# yeniden başlayana (oturumu kapatıp açana) kadar tanınmaz; bu durumda
# `gnome-extensions enable` başarısız olur ve UUID doğrudan GSettings'e eklenir.
set -eu
uuid="$1"

if gnome-extensions enable "$uuid" 2>/dev/null; then
    echo "Eklenti etkin: $uuid"
    exit 0
fi

current=$(gsettings get org.gnome.shell enabled-extensions)
case "$current" in
    *"'$uuid'"*) ;;
    "@as []" | "[]") gsettings set org.gnome.shell enabled-extensions "['$uuid']" ;;
    *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$uuid']" ;;
esac
echo "Eklenti etkinleştirildi; Wayland'da görünmesi için oturumu kapatıp yeniden açın."
