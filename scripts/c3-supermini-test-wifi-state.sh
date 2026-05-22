#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
SKIP_FLASH=0
STRICT_PROBE="${STRICT_PROBE:-0}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-wifi-state.sh [--skip-flash]

Builds/flashes the ESP32-C3 Super Mini reference firmware unless skipped,
installs the Wi-Fi AP diagnostics app, and verifies firmware-reported Wi-Fi
driver state over USB serial. This does not prove external AP visibility or
client joinability.

Set STRICT_PROBE=1 to fail unless probe or station-connect events are observed.
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

PORT="$(resolve_esp_serial_port)"
export ESPFLASH_PORT="$PORT"

send_serial() {
  PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
    --port "$PORT" \
    --timeout 8 \
    send "$1"
}

require_field() {
  local text="$1"
  local expected="$2"
  if [[ "$text" != *"$expected"* ]]; then
    printf 'ERR wifi state test: missing %s\n' "$expected" >&2
    exit 1
  fi
}

field_value() {
  local text="$1"
  local name="$2"
  local line
  line="$(printf '%s\n' "$text" | awk -F= -v key="$name" '$1 == key { value = $2 } END { print value }')"
  printf '%s' "$line"
}

if [[ "$SKIP_FLASH" == "0" ]]; then
  "$ROOT/scripts/c3-supermini-flash.sh"
  sleep 1
fi

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  storage-format

cargo run -p squidc -- app install \
  --port "$PORT" \
  --as wifi-ap-diagnostics \
  "$ROOT/examples/wifi-ap-diagnostics/main.squid"
cargo run -p squidc -- app launch --port "$PORT" wifi-ap-diagnostics
sleep 1

status_started="$(send_serial "WIFI.STATUS")"
printf '%s\n' "$status_started"
require_field "$status_started" "state=started"
require_field "$status_started" "backend=esp"
require_field "$status_started" "active=true"
require_field "$status_started" "mode=ap"
require_field "$status_started" "ssid=SquidScript"
require_field "$status_started" "ip=192.168.4.1"
require_field "$status_started" "driver_started=true"
require_field "$status_started" "configured=true"
require_field "$status_started" "event_ap_start="
require_field "$status_started" "event_ap_probe="
require_field "$status_started" "event_ap_sta_connected="

if [[ "$STRICT_PROBE" == "1" ]]; then
  probe_events="$(field_value "$status_started" "event_ap_probe")"
  connected_events="$(field_value "$status_started" "event_ap_sta_connected")"
  if (( ${probe_events:-0} <= 0 && ${connected_events:-0} <= 0 )); then
    printf '%s\n' 'ERR wifi state test: STRICT_PROBE=1 but no probe/client events observed' >&2
    exit 1
  fi
fi

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  key SELECT
sleep 1

status_stopped="$(send_serial "WIFI.STATUS")"
printf '%s\n' "$status_stopped"
require_field "$status_stopped" "state=stopped"
require_field "$status_stopped" "active=false"
require_field "$status_stopped" "configured=false"

printf 'OK hardware test esp32c3-super-mini wifi state\n'
