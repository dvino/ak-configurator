#!/bin/sh
# Verified, per-user install. No elevated privileges, device access or auto-start changes.
set -eu
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BASE=${XDG_DATA_HOME:-"$HOME/.local/share"}/ak-configurator
BIN_DIR=${XDG_BIN_HOME:-"$HOME/.local/bin"}
DATA_HOME=${XDG_DATA_HOME:-"$HOME/.local/share"}
MANIFEST="$HERE/manifest.json"
[ -x "$HERE/ak-configurator" ] && [ -f "$MANIFEST" ] && [ -f "$HERE/SHA256SUMS" ] || { echo 'Run this from a complete extracted Linux bundle.' >&2; exit 1; }
(cd "$HERE" && sha256sum -c SHA256SUMS >/dev/null) || { echo 'Bundle checksum verification failed.' >&2; exit 1; }
grep -q '"format": "ak-configurator-desktop-candidate-1"' "$MANIFEST" && grep -q '"platform": "linux-' "$MANIFEST" || { echo 'Bundle manifest is not for Linux.' >&2; exit 1; }
EXPECTED=$(sed -n 's/.*"binary_sha256": "\([0-9a-f]*\)".*/\1/p' "$MANIFEST")
ACTUAL=$(sha256sum "$HERE/ak-configurator" | cut -d ' ' -f1)
[ -n "$EXPECTED" ] && [ "$EXPECTED" = "$ACTUAL" ] || { echo 'Executable digest does not match the bundle manifest.' >&2; exit 1; }
mkdir -p "$BASE" "$BIN_DIR" "$DATA_HOME/applications"
CURRENT="$BASE/current"
if [ -e "$CURRENT" ] && [ ! -f "$CURRENT/.ak-configurator-owned" ]; then echo "Refusing to replace unmanaged app directory: $CURRENT" >&2; exit 1; fi
LAUNCHER="$BIN_DIR/ak-configurator"
HAD_LAUNCHER=false
if [ -e "$LAUNCHER" ] || [ -L "$LAUNCHER" ]; then
  [ -L "$LAUNCHER" ] && [ "$(readlink "$LAUNCHER")" = "$CURRENT/ak-configurator" ] || { echo "Refusing to replace unmanaged launcher: $LAUNCHER" >&2; exit 1; }
  HAD_LAUNCHER=true
fi
DESKTOP="$DATA_HOME/applications/ak-configurator.desktop"
if [ -e "$DESKTOP" ] && ! grep -q '^X-AK-Configurator-Owned=true$' "$DESKTOP"; then echo "Refusing to replace unmanaged desktop entry: $DESKTOP" >&2; exit 1; fi
STAGE=$(mktemp -d "$BASE/.stage.XXXXXX")
BACKUP=$(mktemp -d "$BASE/.backup.XXXXXX")
LINK_DIR=$(mktemp -d "$BIN_DIR/.link.XXXXXX")
DESKTOP_DIR=$(mktemp -d "$DATA_HOME/applications/.desktop.XXXXXX")
LINK_STAGE="$LINK_DIR/ak-configurator"
DESKTOP_STAGE="$DESKTOP_DIR/ak-configurator.desktop"
cleanup() {
  if [ -e "$BACKUP/current" ] && [ ! -e "$CURRENT" ]; then
    if ! mv "$BACKUP/current" "$CURRENT"; then echo "Previous app retained at $BACKUP/current" >&2; return; fi
  fi
  rm -rf -- "$STAGE" "$LINK_DIR" "$DESKTOP_DIR"
  [ -e "$BACKUP/current" ] || rmdir "$BACKUP" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM
install -m 0755 "$HERE/ak-configurator" "$STAGE/ak-configurator"
cp -R "$HERE/audio" "$STAGE/audio"
chmod 0755 "$STAGE/audio/cava"
touch "$STAGE/.ak-configurator-owned"
for f in "$HERE"/audio-sources "$HERE"/CAVA-LICENSE "$HERE"/CAVA-PROVENANCE.json "$HERE"/THIRD-PARTY-AUDIO.en.md "$HERE"/THIRD-PARTY-AUDIO.ru.md "$HERE"/offline "$HERE"/INSTALL.en.md "$HERE"/INSTALL.ru.md "$HERE"/firmware-provenance.json; do [ ! -e "$f" ] || cp -R "$f" "$STAGE/"; done
install -m 0755 "$HERE/install-user.sh" "$STAGE/install-user.sh"
install -m 0755 "$HERE/uninstall-user.sh" "$STAGE/uninstall-user.sh"
ln -s "$CURRENT/ak-configurator" "$LINK_STAGE"
printf '[Desktop Entry]\nType=Application\nName=AK Configurator\nExec="%s"\nTerminal=false\nCategories=Utility;\nX-AK-Configurator-Owned=true\n' "$LAUNCHER" > "$DESKTOP_STAGE"
if [ -e "$CURRENT" ]; then mv "$CURRENT" "$BACKUP/current"; fi
if ! mv "$STAGE" "$CURRENT"; then [ ! -e "$BACKUP/current" ] || mv "$BACKUP/current" "$CURRENT"; exit 1; fi
DESKTOP_BACKUP="$DESKTOP_DIR/previous.desktop"
if [ -e "$DESKTOP" ]; then cp -p "$DESKTOP" "$DESKTOP_BACKUP"; fi
if ! mv -f "$LINK_STAGE" "$LAUNCHER"; then
  rm -rf -- "$CURRENT"
  [ ! -e "$BACKUP/current" ] || mv "$BACKUP/current" "$CURRENT"
  exit 1
fi
if ! mv -f "$DESKTOP_STAGE" "$DESKTOP"; then
  if [ -e "$DESKTOP_BACKUP" ]; then mv -f "$DESKTOP_BACKUP" "$DESKTOP"; else rm -f -- "$DESKTOP"; fi
  [ "$HAD_LAUNCHER" = true ] || rm -f -- "$LAUNCHER"
  rm -rf -- "$CURRENT"
  [ ! -e "$BACKUP/current" ] || mv "$BACKUP/current" "$CURRENT"
  exit 1
fi
rm -rf -- "$BACKUP" "$LINK_DIR" "$DESKTOP_DIR"
trap - EXIT HUP INT TERM
echo "Installed for this user at $CURRENT. Launch with: $LAUNCHER"
