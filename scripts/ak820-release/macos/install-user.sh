#!/bin/sh
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP="$HERE/AK-Configurator.app"
MANIFEST="$HERE/manifest.json"
[ -x "$APP/Contents/MacOS/ak-configurator" ] && [ -f "$MANIFEST" ] && [ -f "$HERE/SHA256SUMS" ] || { echo 'Run this from a complete extracted macOS bundle.' >&2; exit 1; }
(cd "$HERE" && shasum -a 256 -c SHA256SUMS >/dev/null) || { echo 'Bundle checksum verification failed.' >&2; exit 1; }
grep -q '"platform": "macos-' "$MANIFEST" || { echo 'Bundle manifest is not for macOS.' >&2; exit 1; }
if pgrep -x ak-configurator >/dev/null 2>&1; then echo 'Close AK Configurator before installing an update.' >&2; exit 1; fi
APP_ROOT=${AK_CONFIGURATOR_APPLICATIONS_DIR:-"$HOME/Applications"}
DEST="$APP_ROOT/AK-Configurator.app"
mkdir -p "$APP_ROOT"
if [ -e "$DEST" ]; then
  [ -f "$DEST/.ak-configurator-owned" ] || { echo "Refusing to replace unmanaged app: $DEST" >&2; exit 1; }
fi
STAGE=$(mktemp -d "$APP_ROOT/.AK-Configurator.stage.XXXXXX")
BACKUP=$(mktemp -d "$APP_ROOT/.AK-Configurator.backup.XXXXXX")
cleanup() {
  if [ -e "$BACKUP/AK-Configurator.app" ] && [ ! -e "$DEST" ]; then
    if ! mv "$BACKUP/AK-Configurator.app" "$DEST"; then echo "Previous app retained at $BACKUP/AK-Configurator.app" >&2; return; fi
  fi
  rm -rf -- "$STAGE"
  [ -e "$BACKUP/AK-Configurator.app" ] || rmdir "$BACKUP" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM
ditto "$APP" "$STAGE/AK-Configurator.app"
touch "$STAGE/AK-Configurator.app/.ak-configurator-owned"
if [ -e "$DEST" ]; then mv "$DEST" "$BACKUP/AK-Configurator.app"; fi
if ! mv "$STAGE/AK-Configurator.app" "$DEST"; then
  [ ! -e "$BACKUP/AK-Configurator.app" ] || mv "$BACKUP/AK-Configurator.app" "$DEST"
  exit 1
fi
rm -rf -- "$BACKUP"
trap - EXIT HUP INT TERM
rm -rf -- "$STAGE" "$BACKUP"
echo "Installed at $DEST; open it from Applications."
