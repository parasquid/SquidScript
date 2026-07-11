#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="file-transfer-regression"
APP_DIR="${ROOT}/tests/hardware/xteink-x4/file-transfer-regression"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-radio-coexistence}"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
PAYLOAD="${PAYLOAD:-${WORK_DIR}/coexistence.binbook}"
HTTP_NAME="${HTTP_NAME:-coexistence-http.binbook}"
BLE_NAME="${BLE_NAME:-coexistence-ble.binbook}"
DEVICE_AP_SSID="${DEVICE_AP_SSID:-SquidScript-X4}"
DEVICE_AP_CONN="${DEVICE_AP_CONN:-squid-x4-radio-coexistence}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
DEVICE="${DEVICE:-}"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-300}"
PREVIOUS_WIFI_CONNECTION=""
gatt_pid=""
ble_pid=""
UV="${UV:-$(command -v uv || true)}"
if [[ -z "${UV}" && -x /home/linuxbrew/.linuxbrew/bin/uv ]]; then
  UV=/home/linuxbrew/.linuxbrew/bin/uv
fi

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-radio-coexistence.sh [--target <id>] [--port <serial-port>] [--device <name-or-address>] [--host-wifi-iface <iface>] [--skip-flash]

Verifies native X4 Wi-Fi/BLE coexistence with the unified upload profile:
GATT remains connected during HTTP upload, HTTP HEAD remains usable during BLE
upload, both copied files match exact size/CRC, BLE reports terminal completion,
and reset releases both radio leases.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --device) DEVICE="${2:-}"; shift 2 ;;
    --host-wifi-iface) HOST_WIFI_IFACE="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

mkdir -p "${WORK_DIR}"

cleanup() {
  set +e
  [[ -n "${ble_pid}" ]] && kill "${ble_pid}" 2>/dev/null
  [[ -n "${gatt_pid}" ]] && kill "${gatt_pid}" 2>/dev/null
  if command -v nmcli >/dev/null 2>&1; then
    nmcli connection down "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-ap-down.out" 2>&1
    nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-ap-delete.out" 2>&1
    if [[ -n "${PREVIOUS_WIFI_CONNECTION}" && -n "${HOST_WIFI_IFACE}" ]]; then
      nmcli connection up "${PREVIOUS_WIFI_CONNECTION}" ifname "${HOST_WIFI_IFACE}" \
        >"${WORK_DIR}/cleanup-wifi-restore.out" 2>&1
    fi
  fi
}
trap cleanup EXIT

wait_for_output() {
  local label="$1"
  local expected="$2"
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- \
      device output --port "${PORT}" >"${out}" 2>&1 || true
    if grep -Fq "${expected}" "${out}"; then
      return 0
    fi
    sleep 0.5
  done
  printf 'Timed out waiting for device output: %s\n' "${expected}" >&2
  capture_device_diagnostics "${label}-timeout"
  return 1
}

detect_wifi_iface() {
  if [[ -n "${HOST_WIFI_IFACE}" ]]; then
    return
  fi
  HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE device status | awk -F: '$2 == "wifi" { print $1; exit }')"
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    printf 'No host Wi-Fi interface found\n' >&2
    exit 1
  fi
}

connect_target_ap() {
  detect_wifi_iface
  PREVIOUS_WIFI_CONNECTION="$(nmcli -g GENERAL.CONNECTION device show "${HOST_WIFI_IFACE}" | tail -1)"
  nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/ap-delete-existing.out" 2>&1 || true
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    nmcli device wifi rescan ifname "${HOST_WIFI_IFACE}" >"${WORK_DIR}/ap-rescan.out" 2>&1 || true
    if nmcli device wifi connect "${DEVICE_AP_SSID}" ifname "${HOST_WIFI_IFACE}" \
      name "${DEVICE_AP_CONN}" >"${WORK_DIR}/ap-connect.out" 2>&1; then
      break
    fi
    sleep 2
  done
  ip -o -4 address show dev "${HOST_WIFI_IFACE}" | awk '{print $4}' \
    >"${WORK_DIR}/ap-address.out"
  grep -Eq '192\.168\.4\.[0-9]+/24' "${WORK_DIR}/ap-address.out"
  curl -fsS --max-time 10 -o /dev/null -I \
    "http://192.168.4.1/upload/coexistence-probe.binbook"
}

hold_gatt_connection() {
  local marker="${WORK_DIR}/gatt-connected"
  rm -f "${marker}"
  BLE_HOLD_MARKER="${marker}" BLE_HOLD_DEVICE="${DEVICE}" \
    "${UV}" run --with bleak --no-project python3 - <<'PY'
import asyncio
import os
from pathlib import Path
from bleak import BleakClient, BleakScanner

SERVICE = "7e57c0de-0001-4a5b-8c6d-0123456789ab"
SELECTOR = os.environ["BLE_HOLD_DEVICE"].lower()

async def main():
    def matches(device, advertisement):
        services = [str(value).lower() for value in advertisement.service_uuids]
        name = (device.name or "").lower()
        address = str(device.address).lower()
        return SERVICE in services and (SELECTOR in name or SELECTOR == address)

    device = await BleakScanner.find_device_by_filter(matches, timeout=15)
    if device is None:
        raise RuntimeError("BLE upload service not found")
    async with BleakClient(device):
        Path(os.environ["BLE_HOLD_MARKER"]).write_text("connected", encoding="ascii")
        await asyncio.sleep(25)

asyncio.run(main())
PY
}

export DBUS_SYSTEM_BUS_ADDRESS="${DBUS_SYSTEM_BUS_ADDRESS:-unix:path=/run/host/run/dbus/system_bus_socket}"
export PATH="/usr/sbin:/sbin:${PATH}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [[ -z "${UV}" ]]; then
  printf 'uv is required for the BLE coexistence check\n' >&2
  exit 1
fi
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

if [[ -z "${DEVICE}" ]]; then
  DEVICE="$(cargo run --quiet -p squidc -- --json target inspect --target "${TARGET_ID}" | \
    python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["name"])')"
fi

create_transfer_binbook_payload "${PAYLOAD}"
read_transfer_payload_meta "${PAYLOAD}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
fi

run_capture package cargo run --quiet -p squidc -- app package "${APP_DIR}" \
  --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture pre-reset cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
run_capture install cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
wait_for_output ready "transfer ready true null"
connect_target_ap

hold_gatt_connection >"${WORK_DIR}/gatt-hold.out" 2>&1 &
gatt_pid=$!
for _ in $(seq 1 20); do
  [[ -f "${WORK_DIR}/gatt-connected" ]] && break
  sleep 1
done
[[ -f "${WORK_DIR}/gatt-connected" ]]
curl -fsS --max-time 60 --http1.1 -H 'Expect:' --upload-file "${PAYLOAD}" \
  "http://192.168.4.1/upload/${HTTP_NAME}" >"${WORK_DIR}/http-upload.out"
wait "${gatt_pid}"
wait_for_output http-copy "upload copy http true null ${SIZE}"

cargo run --quiet -p squidc -- device upload "${PAYLOAD}" --name "${BLE_NAME}" \
  --transport ble --device "${DEVICE}" >"${WORK_DIR}/ble-upload.out" 2>&1 &
ble_pid=$!
head_success=0
head_failure=0
while kill -0 "${ble_pid}" 2>/dev/null; do
  if curl -fsS --max-time 10 -o /dev/null -I \
    "http://192.168.4.1/upload/coexistence-probe.binbook"; then
    head_success=$((head_success + 1))
  else
    head_failure=$((head_failure + 1))
  fi
  sleep 0.5
done
wait "${ble_pid}"
(( head_success >= 3 ))
(( head_success >= head_failure * 4 ))
grep -Fq "uploaded transport=ble name=${BLE_NAME} bytes=${SIZE}" "${WORK_DIR}/ble-upload.out"
wait_for_output ble-copy "upload copy ble true null ${SIZE}"

run_capture http-crc cargo run --quiet -p squidc -- device content-check "${HTTP_NAME}" \
  --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
run_capture ble-crc cargo run --quiet -p squidc -- device content-check "${BLE_NAME}" \
  --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
run_capture active-resources cargo run --quiet -p squidc -- --json device resources \
  --port "${PORT}" >/dev/null
python3 - "${WORK_DIR}/active-resources.out" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))["data"]["resources"]
metrics = {item["key"]: item["value"] for item in data}
assert metrics["radio_active_leases"] == 2
assert metrics["radio_wifi_active"] == 1
assert metrics["radio_ble_active"] == 1
assert metrics["upload_profile_active"] == 1
PY

run_capture reset cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
run_capture reset-resources cargo run --quiet -p squidc -- --json device resources \
  --port "${PORT}" >/dev/null
python3 - "${WORK_DIR}/active-resources.out" "${WORK_DIR}/reset-resources.out" <<'PY'
import json, sys
def metrics(path):
    data = json.load(open(path, encoding="utf-8"))["data"]["resources"]
    return {item["key"]: item["value"] for item in data}
active, reset = metrics(sys.argv[1]), metrics(sys.argv[2])
assert reset["radio_active_leases"] == 0
assert reset["radio_wifi_active"] == 0
assert reset["radio_ble_active"] == 0
assert reset["upload_profile_active"] == 0
assert reset["heap_free_bytes"] > active["heap_free_bytes"]
PY

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
if grep -v '^error=diag\.' "${errors_out}" | grep -q .; then
  printf 'Expected device errors to contain diagnostics only\n' >&2
  exit 1
fi

printf 'OK XTEINK X4 radio coexistence size=%s crc32=%s http_head_success=%s\n' \
  "${SIZE}" "${CRC32}" "${head_success}"
