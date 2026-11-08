#!/bin/bash

set -euo pipefail

PUID="${PUID:-1000}"
PGID="${PGID:-1000}"
CONFIG_DIR=/config

log() {
    echo "[init] $*"
}

if [ "$(id -u)" -ne 0 ]; then
    log "ERROR: the container must start as root; set PUID and PGID instead of changing its user."
    exit 1
fi

umask 0022

DRI_GIDS=""
if [ -d /dev/dri ]; then
    DRI_GIDS="$(find /dev/dri -type c -exec stat -c %g {} + | sort -un | grep -vxE "0|$PGID" | paste -sd, || true)"
fi

if [ -n "$DRI_GIDS" ]; then
    SETPRIV_GROUPS=(--groups "$DRI_GIDS")
else
    SETPRIV_GROUPS=(--clear-groups)
fi

run_as_user() {
    setpriv --reuid="$PUID" --regid="$PGID" "${SETPRIV_GROUPS[@]}" -- "$@"
}

log "user $PUID:$PGID, extra groups: ${DRI_GIDS:-none}"

mkdir -p "$CONFIG_DIR"
find "$CONFIG_DIR" -xdev \( ! -uid "$PUID" -o ! -gid "$PGID" \) -exec chown "$PUID:$PGID" {} + \
    || log "WARNING: could not take ownership of every file under $CONFIG_DIR."

log "CPU: $(awk -F': ' '/^model name/ {print $2; exit}' /proc/cpuinfo)"

for dev in /dev/dri/renderD*; do
    [ -e "$dev" ] || continue
    if ! run_as_user test -r "$dev" -a -w "$dev"; then
        log "WARNING: $dev is not accessible by $PUID:$PGID, hardware encoding will not work on it."
    fi
done

export HOME="$CONFIG_DIR"

while IFS= read -r line; do
    log "$line"
done < <(run_as_user HandBrakeCLI --version 2>&1 >/dev/null | sed -nE 's/^\[[0-9:]+\] ((nvenc|nvdec|vcn|qsv): .*)$/\1/p' || true)

exec setpriv --reuid="$PUID" --regid="$PGID" "${SETPRIV_GROUPS[@]}" -- "$@"
