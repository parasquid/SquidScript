#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

SKIP_FLASH="${SKIP_FLASH:-0}"
SKIP_HOST_SCAN=0
REQUIRE_HOST_SCAN=0
WORK_DIR="${ROOT}/target/hardware-tests/ble-smoke"
DEVICE_NAME="${SQUID_BLE_SMOKE_DEVICE_NAME:-ESP32-C3 Super Mini}"
LOG_TIMEOUT_SECONDS="${SQUID_BLE_SMOKE_LOG_TIMEOUT_SECONDS:-20}"
SCAN_TIMEOUT_SECONDS="${SQUID_BLE_SMOKE_SCAN_TIMEOUT_SECONDS:-15}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-ble-smoke.sh [--skip-flash] [--skip-host-scan] [--require-host-scan]

Builds the ESP32-C3 Super Mini firmware, flashes it unless skipped, verifies
the firmware advertising log over serial, and optionally checks that a host
Bluetooth controller can discover the advertised device name. Host discovery is
best-effort by default; use --require-host-scan when validating the host-side
radio path.

This is a BLE radio smoke check. It does not validate SquidScript BLE file
transfer chunking, staging, or app install.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    --skip-host-scan)
      SKIP_HOST_SCAN=1
      shift
      ;;
    --require-host-scan)
      REQUIRE_HOST_SCAN=1
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

mkdir -p "$WORK_DIR"
export ZEPHYR_PRISTINE="${ZEPHYR_PRISTINE:-always}"
PORT="$(resolve_esp_serial_port)"

if [[ "$SKIP_FLASH" != "1" ]]; then
  cargo run --quiet -p squidc -- target flash --target esp32c3-super-mini
else
  cargo run --quiet -p squidc -- target build --target esp32c3-super-mini >/dev/null
fi

monitor_log="${WORK_DIR}/advertising.log"
monitor_cmd=(cargo run --quiet -p squidc -- target monitor --target esp32c3-super-mini --port "$PORT")
monitor_shell_command="$(printf '%q ' "${monitor_cmd[@]}")"

set +e
if command -v script >/dev/null 2>&1; then
  (
    cd "$ROOT"
    timeout "${LOG_TIMEOUT_SECONDS}s" script -q -e -c "$monitor_shell_command" /dev/null
  ) >"$monitor_log" 2>&1
else
  (
    cd "$ROOT"
    timeout "${LOG_TIMEOUT_SECONDS}s" "${monitor_cmd[@]}"
  ) >"$monitor_log" 2>&1
fi
monitor_status=$?
set -e

if ! grep -Fq "BLE advertising started: ${DEVICE_NAME}" "$monitor_log"; then
  printf 'Expected BLE advertising log not found within %ss on %s.\n' \
    "$LOG_TIMEOUT_SECONDS" "$PORT" >&2
  printf 'Expected: BLE advertising started: %s\n' "$DEVICE_NAME" >&2
  printf 'Monitor log: %s\n' "$monitor_log" >&2
  exit 1
fi

if [[ "$monitor_status" != "0" && "$monitor_status" != "124" ]]; then
  printf 'BLE smoke monitor exited with status %s after log capture.\n' \
    "$monitor_status" >&2
  printf 'Monitor log: %s\n' "$monitor_log" >&2
  exit "$monitor_status"
fi

if [[ "$SKIP_HOST_SCAN" == "1" ]]; then
  printf '%s\n' 'OK BLE smoke serial advertising check passed; host scan skipped'
  exit 0
fi

if ! command -v bluetoothctl >/dev/null 2>&1; then
  printf '%s\n' 'OK BLE smoke serial advertising check passed; host scan skipped because bluetoothctl is unavailable'
  exit 0
fi

if ! bluetoothctl show >/dev/null 2>&1; then
  printf '%s\n' 'OK BLE smoke serial advertising check passed; host scan skipped because no host Bluetooth controller is available'
  exit 0
fi

scan_log="${WORK_DIR}/host-scan.log"
set +e
timeout "${SCAN_TIMEOUT_SECONDS}s" bluetoothctl scan on >"$scan_log" 2>&1
scan_status=$?
set -e

if ! grep -Fq "$DEVICE_NAME" "$scan_log"; then
  if [[ "$REQUIRE_HOST_SCAN" != "1" ]]; then
    printf 'OK BLE smoke serial advertising check passed; host scan did not discover %s within %ss\n' \
      "$DEVICE_NAME" "$SCAN_TIMEOUT_SECONDS"
    printf 'host scan log: %s\n' "$scan_log"
    exit 0
  fi
  printf 'Expected host Bluetooth scan to discover %s within %ss.\n' \
    "$DEVICE_NAME" "$SCAN_TIMEOUT_SECONDS" >&2
  printf 'Scan log: %s\n' "$scan_log" >&2
  exit 1
fi

if [[ "$scan_status" != "0" && "$scan_status" != "124" ]]; then
  printf 'Host Bluetooth scan exited with status %s.\n' "$scan_status" >&2
  printf 'Scan log: %s\n' "$scan_log" >&2
  exit "$scan_status"
fi

printf '%s\n' 'OK BLE smoke advertising check passed'
