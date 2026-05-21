#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
PORT="$(resolve_esp_serial_port)"
export ESPFLASH_PORT="$PORT"
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

format_firmware_storage() {
  PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
    --port "$PORT" \
    storage-format
}

verify_blinky_timer_output() {
  local monitor_output
  monitor_output="$(cargo run -p squidc -- device monitor --max-lines 6)"
  printf '%s\n' "$monitor_output"
  if [[ "$monitor_output" != *'output="blinky ready" false'* ]]; then
    printf '%s\n' 'ERR hardware test: blinky did not report startup output' >&2
    exit 1
  fi
  if [[ "$monitor_output" != *'output="blink" true'* || "$monitor_output" != *'output="blink" false'* ]]; then
    printf '%s\n' 'ERR hardware test: blinky timer did not emit both blink states' >&2
    exit 1
  fi
}

if [[ "$SKIP_FLASH" == "1" ]]; then
  "$ROOT/scripts/c3-supermini-test-reference-firmware.sh" --skip-flash
else
  "$ROOT/scripts/c3-supermini-test-reference-firmware.sh"
fi

format_firmware_storage
cargo run -p squidc -- repl --script tests/repl/hardware-gpio-indicator.session
cargo run -p squidc -- repl --script tests/repl/default-dev.session
"$ROOT/scripts/c3-supermini-test-persistent-app-registry.sh"
format_firmware_storage
"$ROOT/scripts/c3-supermini-test-timer-armed-app.sh"
"$ROOT/scripts/c3-supermini-test-generic-triggered-apps.sh"
cargo run -p squidc -- repl examples/blinky-supermini/main.squid --script tests/repl/blinky-supermini.session
cargo run -p squidc -- run examples/blinky-supermini/main.squid
verify_blinky_timer_output
apps_after_temp_run="$(cargo run -p squidc -- app list)"
printf '%s\n' "$apps_after_temp_run"
if [[ "$apps_after_temp_run" == *'app=blinky-supermini'* ]]; then
  printf '%s\n' 'ERR hardware test: volatile squidc run persisted blinky-supermini' >&2
  exit 1
fi
cargo run -p squidc -- run examples/blinky-supermini/main.squid

printf 'OK hardware test esp32c3-super-mini full sequence\n'
