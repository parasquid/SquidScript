#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-serial-ota}"
IMAGE="${ROOT}/target/riscv32imc-unknown-none-elf/debug/squidscript-fw-x4-ota.bin"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-600}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) shift ;;
    -h|--help) printf 'Usage: %s [--target <id>] [--port <port>] [--skip-flash]\n' "$0"; exit 0 ;;
    *) exit 2 ;;
  esac
done
mkdir -p "${WORK_DIR}"
port_args=()
[[ -n "${PORT}" ]] && port_args=(--port "${PORT}")
[[ -s "${IMAGE}" ]]

run_capture before cargo run --quiet -p squidc -- --json device firmware-info \
  "${port_args[@]}" >/dev/null
run_capture update cargo run --quiet -p squidc -- device firmware-update "${IMAGE}" \
  "${port_args[@]}" >/dev/null
sleep 3
run_capture after cargo run --quiet -p squidc -- --json device firmware-info \
  "${port_args[@]}" >/dev/null
python3 - "${WORK_DIR}/before.out" "${WORK_DIR}/after.out" <<'PY'
import json, sys
before = json.load(open(sys.argv[1], encoding="utf-8"))["data"]
after = json.load(open(sys.argv[2], encoding="utf-8"))["data"]
assert before["activeSlot"] != after["activeSlot"]
assert after["bootState"] == "valid"
PY
cp "${IMAGE}" "${WORK_DIR}/truncated.bin"
truncate -s 4096 "${WORK_DIR}/truncated.bin"
if cargo run --quiet -p squidc -- device firmware-update "${WORK_DIR}/truncated.bin" \
  "${port_args[@]}" >"${WORK_DIR}/truncated.out" 2>&1; then
  printf 'Expected truncated OTA image to be rejected\n' >&2
  exit 1
fi
run_capture final cargo run --quiet -p squidc -- --json device firmware-info \
  "${port_args[@]}" >/dev/null
python3 - "${WORK_DIR}/after.out" "${WORK_DIR}/final.out" <<'PY'
import json, sys
after = json.load(open(sys.argv[1], encoding="utf-8"))["data"]
final = json.load(open(sys.argv[2], encoding="utf-8"))["data"]
assert final["activeSlot"] == after["activeSlot"]
assert final["buildId"] == after["buildId"]
PY
printf 'OK XTEINK X4 serial OTA slot transition and corrupt rejection\n'
