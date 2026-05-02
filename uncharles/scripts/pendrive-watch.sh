#!/usr/bin/env bash
# Continuous pendrive-audit watcher.
#
# Discovers the first external mount under /Volumes/, exports
# PENDRIVE_MOUNT, runs uncharles' pendrive_audit pipeline once, sleeps,
# repeats. The boot volume (Macintosh HD) is a symlink to / on macOS so
# it's filtered automatically; any mounted pendrive shows up as a real
# directory next to it.
#
# Usage from workspace root:
#   uncharles/scripts/pendrive-watch.sh                   # foreground
#   uncharles/scripts/pendrive-watch.sh > out.log 2>&1 &  # background
#
# Ctrl+C (foreground) or `kill <pid>` (background) stops cleanly.
#
# Tunables:
#   INTERVAL_MS              uncharles --interval-ms   (default 500)
#   SLEEP_BETWEEN_CYCLES     seconds between cycles    (default 2)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CONFIG="$ROOT/uncharles/configs/pendrive_audit.yaml"
INTERVAL_MS="${INTERVAL_MS:-500}"
SLEEP_BETWEEN_CYCLES="${SLEEP_BETWEEN_CYCLES:-2}"

discover_pendrive() {
  local v
  for v in /Volumes/*; do
    [ -e "$v" ] || continue
    [ -L "$v" ] && continue        # boot symlink (Macintosh HD -> /)
    [ -d "$v" ] || continue
    case "$(basename "$v")" in
      .*) continue ;;              # hidden
    esac
    echo "$v"
    return 0
  done
  return 1
}

trap 'echo "[pendrive-watch] stopping"; exit 0' INT TERM

echo "[pendrive-watch] start; config=$CONFIG"

while true; do
  mount=$(discover_pendrive || true)
  ts="$(date '+%H:%M:%S')"
  if [ -n "${mount:-}" ]; then
    echo "[pendrive-watch] $ts drive present: $mount"
  else
    echo "[pendrive-watch] $ts no drive"
  fi
  PENDRIVE_MOUNT="${mount:-}" \
    cargo run -q --manifest-path "$ROOT/Cargo.toml" -p uncharles -- \
      --config "$CONFIG" \
      --execute --pretty \
      --interval-ms "$INTERVAL_MS" 2>&1 \
    | sed 's/^/[uncharles] /'
  sleep "$SLEEP_BETWEEN_CYCLES"
done
