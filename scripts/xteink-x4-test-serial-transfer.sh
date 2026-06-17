#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-transfer-serial}"
PAYLOAD_SOURCE="${PAYLOAD_SOURCE:-}"
UPLOAD_NAME="${UPLOAD_NAME:-serial-transfer-smoke.binbook}"
PAYLOAD="${PAYLOAD:-${WORK_DIR}/${UPLOAD_NAME}}"
SKIP_FLASH="${SKIP_FLASH:-0}"
PORT="${PORT:-}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-serial-transfer.sh [--target <id>] [--port <serial-port>] [--skip-flash] [--payload <file.binbook>] [--name <file.binbook>]

Generates a validator-compatible BinBook payload larger than firmware scratch
buffers, streams it over the serial content-put path, and verifies the stored
file by size and CRC32. Use --payload to test a specific existing BinBook.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    --payload) PAYLOAD_SOURCE="${2:-}"; shift 2 ;;
    --name) UPLOAD_NAME="${2:-}"; PAYLOAD="${WORK_DIR}/${UPLOAD_NAME}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

mkdir -p "${WORK_DIR}"
source "${ROOT}/scripts/zephyr-env.sh"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

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
  sleep "${POST_FLASH_SETTLE_SECONDS:-8}"
fi

run_capture reset-before-serial-transfer cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture content-put cargo run --quiet -p squidc -- device content-put "${PAYLOAD}" --name "${UPLOAD_NAME}" --port "${PORT}" >/dev/null
check_out="$(run_capture content-check cargo run --quiet -p squidc -- device content-check "${UPLOAD_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}")"
if ! grep -Fq "content-check ${UPLOAD_NAME} size=${SIZE} crc32=${CRC32}" "${check_out}"; then
  printf 'Expected successful content-check for %s\n' "${UPLOAD_NAME}" >&2
  sed -n '1,120p' "${check_out}" >&2
  exit 1
fi
errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
if [[ -s "${errors_out}" ]]; then
  printf 'Expected device errors to be empty\n' >&2
  sed -n '1,120p' "${errors_out}" >&2
  exit 1
fi
printf 'OK XTEINK X4 serial transfer size=%s crc32=%s\n' "${SIZE}" "${CRC32}"
