#!/bin/sh
set -eu
BASE=${XDG_DATA_HOME:-"$HOME/.local/share"}/ak-configurator
LAUNCHER="${XDG_BIN_HOME:-$HOME/.local/bin}/ak-configurator"
DESKTOP="${XDG_DATA_HOME:-$HOME/.local/share}/applications/ak-configurator.desktop"
if [ -L "$LAUNCHER" ]; then
  target=$(readlink "$LAUNCHER")
  case "$target" in "$BASE/current/ak-configurator") rm -f -- "$LAUNCHER" ;; *) echo 'Leaving unmanaged launcher unchanged.' ;; esac
fi
if [ -f "$DESKTOP" ] && grep -q '^X-AK-Configurator-Owned=true$' "$DESKTOP"; then rm -f -- "$DESKTOP"; fi
if [ -d "$BASE/current" ]; then
  [ -f "$BASE/current/.ak-configurator-owned" ] || { echo 'Refusing to remove unowned app files.' >&2; exit 1; }
  printf 'Remove installed application files in %s/current? [y/N] ' "$BASE"
  IFS= read -r answer
  case "$answer" in y|Y|yes|YES) rm -rf -- "$BASE/current" ;; *) echo 'Application files kept.' ;; esac
fi
echo 'Per-user launcher removed.'
