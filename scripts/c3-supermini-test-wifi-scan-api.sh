#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
SKIP_FLASH=0

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-wifi-scan-api.sh [--skip-flash]

Builds/flashes the ESP32-C3 Super Mini reference firmware unless skipped,
installs the Wi-Fi scan diagnostics app, and verifies wifi.scan() reports a
truthful scan result or concrete driver scan failure without exposing
credentials. The old unsupported scan stub is rejected.
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

require_field() {
  local text="$1"
  local expected="$2"
  if [[ "$text" != *"$expected"* ]]; then
    printf 'ERR wifi scan api test: missing %s\n' "$expected" >&2
    exit 1
  fi
}

reject_field() {
  local text="$1"
  local rejected="$2"
  if [[ "$text" == *"$rejected"* ]]; then
    printf 'ERR wifi scan api test: unexpected %s\n' "$rejected" >&2
    exit 1
  fi
}

redact_scan_output() {
  sed -E \
    -e 's/"wifi ap" "[^"]*"/"wifi ap" "<redacted-ssid>"/g' \
    -e 's/"([[:xdigit:]]{2}:){5}[[:xdigit:]]{2}"/"<redacted-bssid>"/g'
}

require_real_scan_result() {
  local text="$1"
  local scan_line
  scan_line="$(printf '%s\n' "$text" | grep 'wifi scan' | tail -n 1 || true)"
  if [[ -z "$scan_line" ]]; then
    printf 'ERR wifi scan api test: missing wifi scan line\n' >&2
    exit 1
  fi

  if [[ "$scan_line" == *"wifi scan"* && "$scan_line" == *" true null "* ]]; then
    local count
    count="$(printf '%s\n' "$scan_line" | awk '{print $NF}')"
    if ! [[ "$count" =~ ^[0-9]+$ ]] || (( count < 1 )); then
      printf 'ERR wifi scan api test: expected at least one AP, got count=%s\n' "$count" >&2
      exit 1
    fi
    require_field "$text" "wifi ap"
    return
  fi

  if [[ "$scan_line" == *"wifi scan"* && "$scan_line" == *" false "* ]]; then
    reject_field "$scan_line" "unsupported"
    reject_field "$scan_line" "wifi busy"
    require_field "$scan_line" "scan failed"
    return
  fi

  printf 'ERR wifi scan api test: unexpected scan line: %s\n' "$scan_line" >&2
  exit 1
}

PORT="$(resolve_esp_serial_port)"
export ESPFLASH_PORT="$PORT"

if [[ "$SKIP_FLASH" == "0" ]]; then
  "$ROOT/scripts/c3-supermini-flash.sh"
  sleep 1
fi

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  storage-format

cargo run -p squidc -- app install \
  --port "$PORT" \
  --as wifi-scan-diagnostics \
  "$ROOT/examples/wifi-scan-diagnostics/main.squid"
cargo run -p squidc -- app launch --port "$PORT" wifi-scan-diagnostics
sleep 1

output="$(PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  output)"
printf '%s\n' "$output" | redact_scan_output

require_field "$output" "wifi scan"
require_real_scan_result "$output"
reject_field "$output" "password"
reject_field "$output" "credential"

printf 'OK hardware test esp32c3-super-mini wifi scan api\n'
