#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-input-events}"
APP_DIR="${ROOT}/tests/hardware/xteink-x4/key-detector"
PACKAGE="${WORK_DIR}/key-detector.squid.zip"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"

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

run_capture format cargo run --quiet -p squidc -- device storage-format "${port_args[@]}" >/dev/null
run_capture package cargo run --quiet -p squidc -- app package "${APP_DIR}" \
  --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture install cargo run --quiet -p squidc -- app install "${PACKAGE}" "${port_args[@]}" >/dev/null
run_capture launch cargo run --quiet -p squidc -- app launch key-detector "${port_args[@]}" >/dev/null

events=(UP DOWN LEFT RIGHT SELECT BACK POWER POWER.longTap POWER.doubleTap)
count=0
for event in "${events[@]}"; do
  count=$((count + 1))
  run_capture "key-${count}" cargo run --quiet -p squidc -- device key "${event}" \
    "${port_args[@]}" >/dev/null
  event_output="$(run_capture "output-${count}" cargo run --quiet -p squidc -- \
    device output "${port_args[@]}")"
  grep -Fq "output=key ${event} ${count}" "${event_output}"
done
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors "${port_args[@]}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 logical input events count=%s\n' "${count}"
