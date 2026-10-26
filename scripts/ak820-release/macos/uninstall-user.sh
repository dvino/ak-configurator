#!/bin/sh
set -eu
APP="${AK_CONFIGURATOR_APPLICATIONS_DIR:-$HOME/Applications}/AK-Configurator.app"
[ -e "$APP" ] || { echo 'No per-user AK Configurator app found.'; exit 0; }
[ -f "$APP/.ak-configurator-owned" ] || { echo 'Refusing to remove an app without the AK Configurator ownership marker.' >&2; exit 1; }
if pgrep -x ak-configurator >/dev/null 2>&1; then echo 'Close AK Configurator before uninstalling.' >&2; exit 1; fi
printf 'Remove %s? [y/N] ' "$APP"
IFS= read -r answer
case "$answer" in y|Y|yes|YES) rm -rf -- "$APP"; echo 'Removed.' ;; *) echo 'Kept.' ;; esac
