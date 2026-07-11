#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-planned-sleep}"
PACKAGE="${WORK_DIR}/planned-sleep.squid.zip"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-45}"

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
run_capture package cargo run --quiet -p squidc -- app package "${ROOT}/examples/planned-sleep" \
  --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture install cargo run --quiet -p squidc -- app install "${PACKAGE}" "${port_args[@]}" >/dev/null
run_capture launch cargo run --quiet -p squidc -- app launch planned-sleep "${port_args[@]}" >/dev/null
run_capture sleep cargo run --quiet -p squidc -- device key POWER "${port_args[@]}" >/dev/null

deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
while (( SECONDS < deadline )); do
  timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- device output \
    "${port_args[@]}" >"${WORK_DIR}/wake-output.out" 2>&1 || true
  grep -Fq 'output=start wake 1' "${WORK_DIR}/wake-output.out" && break
  sleep 1
done
grep -Fq 'output=start wake 1' "${WORK_DIR}/wake-output.out"
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors "${port_args[@]}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 planned sleep timer wake restored state\n'
