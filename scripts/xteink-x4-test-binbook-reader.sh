#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="binbook-reader"
APP_DIR="${ROOT}/examples/binbook-reader"
WORK_DIR="${ROOT}/target/hardware-tests/xteink-x4-binbook-reader"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
BOOK_ONE_PROVIDED="${BOOK_ONE+x}"
BOOK_TWO_PROVIDED="${BOOK_TWO+x}"
BOOK_ONE="${BOOK_ONE:-${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke/books/sample.binbook}"
BOOK_TWO="${BOOK_TWO:-${ROOT}/tests/hardware/xiao-esp32c3/epaper-fast-redraw-smoke/books/sample.binbook}"
BOOK_ONE_NAME="${BOOK_ONE_NAME:-reader-one.binbook}"
BOOK_TWO_NAME="${BOOK_TWO_NAME:-reader-two.binbook}"
PORT="${PORT:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-binbook-reader.sh [--target <id>] [--port <serial-port>] [--skip-flash]

Flashes XTEINK firmware, installs two BinBooks into the books library, installs
the BinBook reader app, drives selection/resume/menu flows through serial
logical key events, and checks drawlog/errors.
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
if [[ -z "${BOOK_ONE_PROVIDED}" && -z "${BOOK_TWO_PROVIDED}" ]]; then
  BOOK_ONE="${WORK_DIR}/reader-one.generated.binbook"
  BOOK_TWO="${WORK_DIR}/reader-two.generated.binbook"
  python3 "${ROOT}/scripts/generate-test-binbook.py" "${BOOK_ONE}"
  python3 "${ROOT}/scripts/generate-test-binbook.py" "${BOOK_TWO}"
fi
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
  printf 'Timed out waiting for %s\n' "${expected}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

run_key() {
  local label="$1"
  local key="$2"
  local out="${WORK_DIR}/${label}.out"
  for attempt in $(seq 1 20); do
    printf '%s: cargo run --quiet -p squidc -- device key %s --port %s\n' \
      "${HARDWARE_COMMAND_LABEL}" "${key}" "${PORT}" >&2
    if timeout 20s cargo run --quiet -p squidc -- device key "${key}" --port "${PORT}" >"${out}" 2>&1; then
      return 0
    fi
    if grep -Fq "busy (-16)" "${out}"; then
      sleep 2
      continue
    fi
    printf 'Command failed while sending key %s\n' "${key}" >&2
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
    capture_device_diagnostics "${label}-failure"
    exit 1
  done
  printf 'Timed out sending key %s after retries\n' "${key}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  capture_device_diagnostics "${label}-failure"
  exit 1
}

move_library_selection_to() {
  local expected="$1"
  local out="$2"
  for _ in $(seq 1 32); do
    local target_index=""
    local library_top=""
    local library_selected=""
    target_index="$(awk -v expected="${expected}" '$1 == "output=book" && $3 == expected { print $2; exit }' "${out}")"
    library_top="$(awk '$1 == "output=library" { print $3; exit }' "${out}")"
    library_selected="$(awk '$1 == "output=library" { print $4; exit }' "${out}")"
    if [[ -n "${target_index}" && -n "${library_top}" && -n "${library_selected}" ]]; then
      if ((library_top + library_selected == target_index)); then
        return 0
      fi
    fi
    run_key library-down DOWN
    out="$(wait_for_contains output-library-scroll "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
  done
  printf 'Timed out moving library selection to %s\n' "${expected}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

if [[ ! -s "${BOOK_ONE}" ]]; then
  printf 'Book fixture not found or empty: %s\n' "${BOOK_ONE}" >&2
  exit 1
fi
if [[ ! -s "${BOOK_TWO}" ]]; then
  printf 'Book fixture not found or empty: %s\n' "${BOOK_TWO}" >&2
  exit 1
fi

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep "${POST_FLASH_SETTLE_SECONDS:-8}"
fi

run_capture reset-before-reader cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture content-put-one cargo run --quiet -p squidc -- device content-put "${BOOK_ONE}" --name "${BOOK_ONE_NAME}" --port "${PORT}" >/dev/null
run_capture content-put-two cargo run --quiet -p squidc -- device content-put "${BOOK_TWO}" --name "${BOOK_TWO_NAME}" --port "${PORT}" >/dev/null
book_one_size="$(stat -c '%s' "${BOOK_ONE}")"
book_two_size="$(stat -c '%s' "${BOOK_TWO}")"
book_one_crc="$(python3 -c 'import sys, zlib; print(format(zlib.crc32(open(sys.argv[1], "rb").read()) & 0xffffffff, "08x"))' "${BOOK_ONE}")"
book_two_crc="$(python3 -c 'import sys, zlib; print(format(zlib.crc32(open(sys.argv[1], "rb").read()) & 0xffffffff, "08x"))' "${BOOK_TWO}")"
run_capture content-check-one cargo run --quiet -p squidc -- device content-check "${BOOK_ONE_NAME}" --size "${book_one_size}" --crc32 "${book_one_crc}" --port "${PORT}" >/dev/null
run_capture content-check-two cargo run --quiet -p squidc -- device content-check "${BOOK_TWO_NAME}" --size "${book_two_size}" --crc32 "${book_two_crc}" --port "${PORT}" >/dev/null
run_capture package-reader cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture install-reader cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch-reader cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null

library_out="$(wait_for_contains output-library "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
move_library_selection_to "${BOOK_TWO_NAME}" "${library_out}"

run_key open-selected SELECT
reader_out="$(wait_for_contains output-reader "reader ${BOOK_TWO_NAME}" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${reader_out}" "${BOOK_TWO_NAME}"

run_key next-page RIGHT
page_out="$(wait_for_contains output-page "reader ${BOOK_TWO_NAME} 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${page_out}" "${BOOK_TWO_NAME}"

run_capture reset-interrupted-reader cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture relaunch-reader cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
resume_out="$(wait_for_contains output-resume "reader ${BOOK_TWO_NAME} 1" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${resume_out}" "${BOOK_TWO_NAME}"
reader_drawlog_out="$(run_capture drawlog-reader cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${reader_drawlog_out}" "draw=binbook"
assert_file_contains "${reader_drawlog_out}" "mode=full"

run_key open-menu BACK
menu_out="$(wait_for_contains output-menu "menu ${BOOK_TWO_NAME}" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${menu_out}" "${BOOK_TWO_NAME}"
run_key menu-down-1 DOWN
run_key menu-down-2 DOWN
run_key menu-library SELECT
library_again_out="$(wait_for_contains output-library-again "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${library_again_out}" "library"

run_capture reset-from-library cargo run --quiet -p squidc -- device reset --port "${PORT}" >/dev/null
sleep "${POST_RESET_SETTLE_SECONDS:-2}"
run_capture relaunch-from-library cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
nonresume_out="$(wait_for_contains output-nonresume "library" cargo run --quiet -p squidc -- device output --port "${PORT}")"
assert_file_contains "${nonresume_out}" "library"

drawlog_out="$(run_capture drawlog cargo run --quiet -p squidc -- device drawlog --port "${PORT}")"
assert_file_contains "${drawlog_out}" "${BOOK_TWO_NAME}"
errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
assert_file_empty_command "${errors_out}"
resources_out="$(run_capture resources cargo run --quiet -p squidc -- device resources --port "${PORT}")"
assert_file_contains "${resources_out}" "proto_stack_unused_bytes"
assert_file_contains "${resources_out}" "vm_stack_unused_bytes"

printf '%s\n' 'OK XTEINK X4 BinBook reader selection hardware check passed'
