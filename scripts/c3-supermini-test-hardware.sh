#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKIP_FLASH=0

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-hardware.sh [--skip-flash]

Runs the ESP32-C3 Super Mini hardware target checks sequentially and leaves the
blinky app running last so the onboard LED remains visibly active.

Set ESPFLASH_PORT=/path/to/tty when auto-detection is not enough.
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

if [[ "$SKIP_FLASH" == "1" ]]; then
  "$ROOT/scripts/c3-supermini-test-reference-firmware.sh" --skip-flash
else
  "$ROOT/scripts/c3-supermini-test-reference-firmware.sh"
fi

cargo run -p squidc -- repl --script tests/repl/hardware-gpio-status-led.session
cargo run -p squidc -- repl --script tests/repl/default-dev.session
"$ROOT/scripts/c3-supermini-test-timer-armed-app.sh"
"$ROOT/scripts/c3-supermini-test-generic-triggered-apps.sh"
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
cargo run -p squidc -- run examples/blinky-supermini/main.squid
cargo run -p squidc -- device monitor --max-lines 4

printf 'OK hardware test esp32c3-super-mini full sequence\n'
