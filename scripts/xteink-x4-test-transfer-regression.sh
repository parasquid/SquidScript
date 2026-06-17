#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
DEVICE="${DEVICE:-}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
PAYLOAD_SOURCE="${PAYLOAD_SOURCE:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
RUN_SERIAL="${RUN_SERIAL:-1}"
RUN_HTTP="${RUN_HTTP:-1}"
RUN_BLE="${RUN_BLE:-1}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-transfer-regression.sh [--target <id>] [--port <serial-port>] [--device <name-or-address>] [--host-wifi-iface <iface>] [--skip-flash] [--payload <file.binbook>] [--serial-only|--http-only|--ble-only]

Runs the XTEINK X4 serial, HTTP, and BLE large-file transfer regressions
sequentially. Each transport uploads a validator-compatible generated BinBook
payload and verifies the stored file by device-side size and CRC32. Use
--payload to test a specific existing BinBook.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --device) DEVICE="${2:-}"; shift 2 ;;
    --host-wifi-iface) HOST_WIFI_IFACE="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    --payload) PAYLOAD_SOURCE="${2:-}"; shift 2 ;;
    --serial-only) RUN_SERIAL=1; RUN_HTTP=0; RUN_BLE=0; shift ;;
    --http-only) RUN_SERIAL=0; RUN_HTTP=1; RUN_BLE=0; shift ;;
    --ble-only) RUN_SERIAL=0; RUN_HTTP=0; RUN_BLE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

source "${ROOT}/scripts/zephyr-env.sh"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

if [[ "${RUN_SERIAL}" == "1" ]]; then
  serial_args=(--target "${TARGET_ID}" --port "${PORT}" --payload "${PAYLOAD_SOURCE}" --name serial-transfer-smoke.binbook)
  if [[ "${SKIP_FLASH}" == "1" ]]; then
    serial_args+=(--skip-flash)
  fi
  scripts/xteink-x4-test-serial-transfer.sh "${serial_args[@]}"
  SKIP_FLASH=1
elif [[ "${SKIP_FLASH}" != "1" ]]; then
  WORK_DIR="${ROOT}/target/hardware-tests/xteink-x4-transfer-regression-flash" \
    scripts/xteink-x4-test-serial-transfer.sh --target "${TARGET_ID}" --port "${PORT}" --payload "${PAYLOAD_SOURCE}" --name serial-transfer-flash-smoke.binbook
  SKIP_FLASH=1
fi

if [[ "${RUN_HTTP}" == "1" ]]; then
  http_args=(--target "${TARGET_ID}" --port "${PORT}" --skip-flash --payload "${PAYLOAD_SOURCE}" --name http-transfer-smoke.binbook)
  if [[ -n "${HOST_WIFI_IFACE}" ]]; then
    http_args+=(--host-wifi-iface "${HOST_WIFI_IFACE}")
  fi
  scripts/xteink-x4-test-http-transfer.sh "${http_args[@]}"
fi

if [[ "${RUN_BLE}" == "1" ]]; then
  ble_args=(--target "${TARGET_ID}" --port "${PORT}" --skip-flash --payload "${PAYLOAD_SOURCE}" --name ble-transfer-smoke.binbook)
  if [[ -n "${DEVICE}" ]]; then
    ble_args+=(--device "${DEVICE}")
  fi
  scripts/xteink-x4-test-ble-transfer.sh "${ble_args[@]}"
fi

printf '%s\n' 'OK XTEINK X4 transfer regression suite passed'
