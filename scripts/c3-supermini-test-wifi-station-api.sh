#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
SKIP_FLASH=0
STRICT_STA_CONNECT="${STRICT_STA_CONNECT:-1}"
ENV_FILE="${HWTEST_ENV_FILE:-$HOME/.env}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-wifi-station-api.sh [--skip-flash]

Builds/flashes the ESP32-C3 Super Mini reference firmware unless skipped,
provisions volatile Wi-Fi profile "dev" from HWTEST_STA_SSID and
HWTEST_STA_PASSWORD, installs the station diagnostics app, and verifies the
public station API path reports truthful status over USB serial.

With provisioned credentials, the default requires connected=true. Set
STRICT_STA_CONNECT=0 only when deliberately collecting driver diagnostics from
an environment where association is expected to fail.
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

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

: "${HWTEST_STA_SSID:?set HWTEST_STA_SSID to the 2.4 GHz station network name}"
: "${HWTEST_STA_PASSWORD:?set HWTEST_STA_PASSWORD in the environment or HWTEST_ENV_FILE}"

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
    printf 'ERR wifi station api test: missing %s\n' "$expected" >&2
    exit 1
  fi
}

reject_field() {
  local text="$1"
  local rejected="$2"
  if [[ "$text" == *"$rejected"* ]]; then
    printf 'ERR wifi station api test: unexpected %s\n' "$rejected" >&2
    exit 1
  fi
}

if [[ "$SKIP_FLASH" == "0" ]]; then
  "$ROOT/scripts/c3-supermini-flash.sh"
  sleep 1
fi

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  storage-format
PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  wifi-profile dev "$HWTEST_STA_SSID" "$HWTEST_STA_PASSWORD"

cargo run -p squidc -- app install \
  --port "$PORT" \
  --as wifi-station-diagnostics \
  "$ROOT/examples/wifi-station-diagnostics/main.squid"
cargo run -p squidc -- app launch --port "$PORT" wifi-station-diagnostics
sleep 1

status_started="$(send_serial "WIFI.STATUS")"
printf '%s\n' "$status_started"
require_field "$status_started" "profile=dev"
require_field "$status_started" "connected="
require_field "$status_started" "scan_matches="
require_field "$status_started" "disconnect_reason="
require_field "$status_started" "driver_mode=sta"
reject_field "$status_started" "station unavailable"
if [[ "$STRICT_STA_CONNECT" == "1" ]]; then
  require_field "$status_started" "connected=true"
fi

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  key SELECT
sleep 1

status_stopped="$(send_serial "WIFI.STATUS")"
printf '%s\n' "$status_stopped"
require_field "$status_stopped" "connected=false"

printf 'OK hardware test esp32c3-super-mini wifi station api\n'
