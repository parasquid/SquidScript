#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="file-transfer-regression"
APP_DIR="${ROOT}/tests/hardware/xteink-x4/file-transfer-regression"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-transfer-ble}"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
PAYLOAD_SOURCE="${PAYLOAD_SOURCE:-}"
UPLOAD_NAME="${UPLOAD_NAME:-ble-transfer-smoke.binbook}"
PAYLOAD="${PAYLOAD:-${WORK_DIR}/${UPLOAD_NAME}}"
DEVICE="${DEVICE:-}"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-ble-transfer.sh [--target <id>] [--port <serial-port>] [--device <name-or-address>] [--skip-flash] [--payload <file.binbook>] [--name <file.binbook>]

Installs the X4 transfer receiver, streams a validator-compatible generated
BinBook payload over BLE file transfer, and verifies the copied file by size and
CRC32. Use --payload to test a specific existing BinBook.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --device) DEVICE="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    --payload) PAYLOAD_SOURCE="${2:-}"; shift 2 ;;
    --name) UPLOAD_NAME="${2:-}"; PAYLOAD="${WORK_DIR}/${UPLOAD_NAME}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

mkdir -p "${WORK_DIR}"

wait_for_contains() {
  local label="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${COMMAND_TIMEOUT_SECONDS}s" "$@" >"${out}" 2>&1 || true
    if grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.5
  done
  printf 'Timed out waiting for %s\n' "${expected}" >&2
  sed -n '1,160p' "${out}" >&2 || true
  capture_device_diagnostics "${label}-timeout"
  exit 1
}

source "${ROOT}/scripts/zephyr-env.sh"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

if [[ -z "${DEVICE}" ]]; then
  TARGET_INSPECT_JSON="${WORK_DIR}/target-inspect.json"
  cargo run --quiet -p squidc -- --json target inspect --target "${TARGET_ID}" >"${TARGET_INSPECT_JSON}"
  DEVICE="$(python3 - "${TARGET_INSPECT_JSON}" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["data"]["name"])
PY
)"
fi

if [[ -n "${PAYLOAD_SOURCE}" ]]; then
  cp "${PAYLOAD_SOURCE}" "${PAYLOAD}"
  write_transfer_payload_meta "${PAYLOAD}"
else
  create_transfer_binbook_payload "${PAYLOAD}"
fi
read_transfer_payload_meta "${PAYLOAD}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
fi

run_capture package-transfer cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture install-transfer cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch-transfer cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
wait_for_contains output-ready "transfer ready" cargo run --quiet -p squidc -- device output --port "${PORT}" >/dev/null
run_capture ble-put cargo run --quiet -p squidc -- device ble-put "${DEVICE}" "${PAYLOAD}" --name "${UPLOAD_NAME}" >/dev/null
wait_for_contains output-copy "ble copy true null" cargo run --quiet -p squidc -- device output --port "${PORT}" >/dev/null
run_capture content-check cargo run --quiet -p squidc -- device content-check "${UPLOAD_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
if [[ -s "${errors_out}" ]]; then
  printf 'Expected device errors to be empty\n' >&2
  sed -n '1,120p' "${errors_out}" >&2
  exit 1
fi
printf 'OK XTEINK X4 BLE transfer size=%s crc32=%s\n' "${SIZE}" "${CRC32}"
