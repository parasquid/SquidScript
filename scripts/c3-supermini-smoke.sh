#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-/dev/ttyACM0}"
SKIP_FLASH=0

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-smoke.sh [--skip-flash]

Builds and flashes the ESP32-C3 Super Mini reference firmware, installs the
headless counter SQBC fixture, then verifies run/key/state/trace behavior over
USB serial.

Set ESPFLASH_PORT=/path/to/tty to override the default /dev/ttyACM0.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$SKIP_FLASH" == "0" ]]; then
  "$ROOT/scripts/c3-supermini-flash.sh"
  sleep 1
fi

"$ROOT/scripts/c3-supermini-install-sqbc.sh"
PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  smoke

printf 'OK smoke esp32c3-super-mini reference firmware\n'
