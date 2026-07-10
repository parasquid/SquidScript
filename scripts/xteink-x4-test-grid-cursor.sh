#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="grid-cursor"
APP_DIR="${ROOT}/examples/grid-cursor"
WORK_DIR="${ROOT}/target/hardware-tests/xteink-x4-grid-cursor"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-grid-cursor.sh [--target <id>] [--port <serial-port>] [--skip-flash]

Flashes XTEINK firmware, installs the grid-cursor app, drives the cursor
through all four directions, asserts fast1bpp refresh, resets mid-run to
verify lifecycle (no stale differential), and checks errors/resources.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET_ID="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "${WORK_DIR}"
source "${ROOT}/scripts/zephyr-env.sh"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

wait_for_contains() {
  local out
  if out="$(try_wait_for_contains "$@")"; then
    printf '%s\n' "${out}"
    return 0
  fi
  local label="$1"
  local expected="$2"
  local file="${WORK_DIR}/${label}.out"
  printf 'Timed out waiting for %s\n' "${expected}" >&2
  printf '%s\n' "--- ${file} ---" >&2
  sed -n '1,200p' "${file}" >&2
  exit 1
}

try_wait_for_contains() {
  local label="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while ((SECONDS < deadline)); do
    timeout "${COMMAND_TIMEOUT_SECONDS}s" "$@" >"${out}" 2>&1
    if grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.2
  done
  return 1
}

run_key() {
  local label="$1"
  local key="$2"
  local out="${WORK_DIR}/${label}.out"
  for attempt in $(seq 1 20); do
    printf '%s: cargo run --quiet -p squidc -- device key %s --port %s\n' \
      "${HARDWARE_COMMAND_LABEL}" "${key}" "${PORT}" >&2
    if timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- device key "${key}" --port "${PORT}" >"${out}" 2>&1; then
      return 0
    fi
    if grep -Fq "busy (-16)" "${out}"; then
      sleep 2
      return 0
    fi
    printf 'Command failed while sending key %s\n' "${key}" >&2
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
    exit 1
  done
  printf 'Timed out sending key %s after retries\n' "${key}" >&2
  exit 1
}

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep "${POST_FLASH_SETTLE_SECONDS:-8}"
fi

run_capture reset-before-grid cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture package-grid cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture install-grid cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch-grid cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null

cursor_out="$(wait_for_contains output-cursor "cursor" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 0 0"

run_key cursor-down DOWN
cursor_out="$(wait_for_contains output-down "cursor 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 1 0"

run_key cursor-right RIGHT
cursor_out="$(wait_for_contains output-right "cursor 1 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 1 1"

drawlog_out="$(run_capture drawlog-grid cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${drawlog_out}" "refreshMode fast1bpp"

run_key cursor-up UP
cursor_out="$(wait_for_contains output-up "cursor 0 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 0 1"

run_key cursor-left LEFT
cursor_out="$(wait_for_contains output-left "cursor 0 0" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 0 0"

sleep "${POST_RESET_SETTLE_SECONDS:-2}"
for reset_attempt in $(seq 1 5); do
  if timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- device reset --port "${PORT}" >"${WORK_DIR}/reset-mid-run.out" 2>&1; then
    break
  fi
  if grep -Fq "busy (-16)" "${WORK_DIR}/reset-mid-run.out"; then
    sleep 2
    continue
  fi
  printf 'reset attempt %d failed:\n' "${reset_attempt}" >&2
  sed -n '1,20p' "${WORK_DIR}/reset-mid-run.out" >&2
  if [[ "${reset_attempt}" == "5" ]]; then
    capture_device_diagnostics "reset-mid-run-failure"
    exit 1
  fi
  sleep 3
done
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture relaunch-grid cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
cursor_out="$(wait_for_contains output-after-reset "cursor" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 0 0"

run_key cursor-down-after-reset DOWN
cursor_out="$(wait_for_contains output-down-after-reset "cursor 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${cursor_out}" "cursor 1 0"

drawlog_after_reset="$(run_capture drawlog-after-reset cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${drawlog_after_reset}" "refreshMode fast1bpp"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
assert_file_empty_command "${errors_out}"
resources_out="$(run_capture resources cargo run --quiet -p squidc -- device resources --port "${PORT}")"
assert_file_contains "${resources_out}" "serial_buffer_bytes"
assert_file_contains "${resources_out}" "runtime_static_bytes"

printf '%s\n' 'OK XTEINK X4 grid cursor hardware check passed'
