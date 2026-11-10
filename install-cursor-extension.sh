#!/bin/sh
# Installs the optional GNOME Shell extension that lets the mascot see the mouse pointer
# on a Wayland session. Remove it with: gnome-extensions uninstall claude-mascot-cursor@claude.local
set -e
uuid=claude-mascot-cursor@claude.local
dest="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$uuid"
mkdir -p "$dest"
cp "$(dirname "$0")/gnome-extension/$uuid/"* "$dest/"
if gnome-extensions enable "$uuid" 2>/dev/null; then
    echo "Enabled. The companion can now see the pointer."
else
    echo "Installed. Log out and back in once (GNOME only discovers new extensions at login),"
    echo "then run: gnome-extensions enable $uuid"
fi
