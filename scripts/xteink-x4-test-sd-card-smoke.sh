#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"
TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-sd-card-smoke}"
PAYLOAD="${WORK_DIR}/sd-persistence.binbook"
NAME="sd-persistence.binbook"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-300}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    -h|--help) printf 'Usage: %s [--target <id>] [--port <port>] [--skip-flash]\n' "$0"; exit 0 ;;
    *) exit 2 ;;
  esac
done
mkdir -p "${WORK_DIR}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"
create_transfer_binbook_payload "${PAYLOAD}"
read_transfer_payload_meta "${PAYLOAD}"

if [[ "${SKIP_FLASH}" != 1 ]]; then
  run_capture flash cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
fi
run_capture format-before cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture upload cargo run --quiet -p squidc -- device content-put "${PAYLOAD}" \
  --name "${NAME}" --port "${PORT}" >/dev/null
run_capture check-before cargo run --quiet -p squidc -- device content-check "${NAME}" \
  --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
run_capture format-after cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture check-after cargo run --quiet -p squidc -- device content-check "${NAME}" \
  --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 SD content survived internal storage format size=%s crc32=%s\n' \
  "${SIZE}" "${CRC32}"
