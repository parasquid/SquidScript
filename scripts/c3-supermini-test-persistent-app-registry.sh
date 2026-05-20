#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
PORT="$(resolve_esp_serial_port)"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-persistent-app-registry.sh

Formats SquidScript app storage, installs the headless counter fixture as main,
mutates saved state, resets the ESP32-C3, then verifies APP.LIST, automatic
root main boot, and persistent state restore.

Set ESPFLASH_PORT=/path/to/tty when auto-detection is not enough.
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
  "")
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  storage-format

cargo run -p squidc -- app install \
  --port "$PORT" \
  --as main \
  "$ROOT/compiler/rust/fixtures/conformance/headless_counter.squid"

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  run-app main
PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  key SELECT
PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  key SELECT

STATE_BEFORE_RESET="$(PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  state)"
printf '%s\n' "$STATE_BEFORE_RESET"
if [[ "$STATE_BEFORE_RESET" != *"count=2"* ]]; then
  printf 'persistent state setup did not save count=2 before chip reset\n' >&2
  exit 1
fi

if command -v espflash >/dev/null 2>&1; then
  ESPFLASH_BIN="$(command -v espflash)"
elif [[ -x "$HOME/.cargo/bin/espflash" ]]; then
  ESPFLASH_BIN="$HOME/.cargo/bin/espflash"
else
  printf 'espflash is required for the reset phase. Install with: cargo install espflash --locked\n' >&2
  exit 1
fi

"$ESPFLASH_BIN" reset --chip esp32c3 --port "$PORT"
sleep 1

APPS_OUTPUT="$(PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  app-list)"
printf '%s\n' "$APPS_OUTPUT"
if [[ "$APPS_OUTPUT" != *"app=main"* ]]; then
  printf 'persistent registry did not contain app=main after chip reset\n' >&2
  exit 1
fi

STATE_OUTPUT="$(PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  state)"
printf '%s\n' "$STATE_OUTPUT"
if [[ "$STATE_OUTPUT" != *"started=1"* ]]; then
  printf 'root main did not auto-run app.start after chip reset\n' >&2
  exit 1
fi
if [[ "$STATE_OUTPUT" != *"count=2"* ]]; then
  printf 'state.load did not restore count=2 after chip reset\n' >&2
  exit 1
fi

printf 'OK hardware test esp32c3-super-mini persistent app registry\n'
