#!/usr/bin/env sh
# Ajoute Mouflon au menu des applications de ce compte (rien n'est écrit
# ailleurs que dans ton dossier personnel). Pour l'enlever :
#   rm ~/.local/share/applications/mouflon.desktop ~/.local/share/icons/hicolor/256x256/apps/mouflon.png
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONS="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
mkdir -p "$APPS" "$ICONS"
cp "$HERE/mouflon.png" "$ICONS/mouflon.png"
sed "s|^Exec=.*|Exec=\"$HERE/mouflon\"|" "$HERE/mouflon.desktop" > "$APPS/mouflon.desktop"
echo "Mouflon est dans le menu des applications. / Mouflon is now in your applications menu."
