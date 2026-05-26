#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"

WORK_DIR="${ROOT}/target/hardware-tests/gpio9-raw-probe"
GPIO9_RAW_APP="${ROOT}/tests/hardware/c3-supermini/gpio9-raw-probe/main.squid"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-20}"

mkdir -p "${WORK_DIR}"

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
  local command_name="$3"
  shift 3
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))

  while (( SECONDS < deadline )); do
    if timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@" >"${out}" 2>&1 &&
      grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for %s in %s\n' "${expected}" "${command_name}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

run_capture install-gpio9-raw cargo run --quiet -p squidc -- app install "${GPIO9_RAW_APP}" >/dev/null

printf '%s\n' 'Release the ESP32-C3 Super Mini BOOT/GPIO9 button now.' >&2
run_capture reset-before-released cargo run --quiet -p squidc -- device reset >/dev/null
run_capture launch-gpio9-raw-released cargo run --quiet -p squidc -- app launch gpio9-raw-probe >/dev/null
released_out="$(wait_for_contains output-gpio9-released "output=gpio9 true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${released_out}" "output=gpio9 true"

printf '%s\n' 'Keep the ESP32-C3 Super Mini BOOT/GPIO9 button released for reset.' >&2
run_capture reset-before-held cargo run --quiet -p squidc -- device reset >/dev/null
printf '%s\n' 'Press and hold the ESP32-C3 Super Mini BOOT/GPIO9 button now.' >&2
run_capture launch-gpio9-raw-held cargo run --quiet -p squidc -- app launch gpio9-raw-probe >/dev/null
held_out="$(wait_for_contains output-gpio9-held "output=gpio9 false" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${held_out}" "output=gpio9 false"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK ESP32-C3 GPIO9 raw held/released probe passed'
