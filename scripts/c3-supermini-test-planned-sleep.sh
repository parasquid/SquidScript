#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/planned-sleep"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-45}"
PLANNED_SLEEP_APP="${ROOT}/tests/hardware/c3-supermini/planned-sleep/main.squid"

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

wait_for_contains_tolerant() {
  local label="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))

  while (( SECONDS < deadline )); do
    if timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@" >"${out}" 2>&1 &&
      grep -Fq "${expected}" "${out}"; then
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

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-planned-sleep cargo run --quiet -p squidc -- app install "${PLANNED_SLEEP_APP}" >/dev/null
run_capture launch-planned-sleep cargo run --quiet -p squidc -- app launch planned-sleep >/dev/null

boot_out="$(wait_for_contains_tolerant output-launch "output=start launch 0" \
  cargo run --quiet -p squidc -- device output)"
assert_file_contains "${boot_out}" "output=start launch 0"

run_capture key-select-sleep cargo run --quiet -p squidc -- device key SELECT >/dev/null

wake_out="$(wait_for_contains_tolerant output-wake "output=start wake 1" \
  cargo run --quiet -p squidc -- device output)"
assert_file_contains "${wake_out}" "output=start wake 1"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr planned sleep SquidScript hardware check passed'
