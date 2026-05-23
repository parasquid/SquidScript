#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/app-state"
STATE_APP="${ROOT}/tests/hardware/c3-supermini/state-counter/main.squid"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware app state: %s\n' "$*" >&2
  "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

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

assert_state_nonempty() {
  local file="$1"
  assert_file_contains "${file}" "state="
  if grep -Fxq "state=" "${file}"; then
    printf 'Expected %s to contain saved app state bytes\n' "${file}" >&2
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

  for _ in $(seq 1 40); do
    "$@" >"${out}" 2>&1
    if grep -Fq "${expected}" "${out}"; then
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

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-state-counter cargo run --quiet -p squidc -- app install "${STATE_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=state-counter"

run_capture launch-state-counter cargo run --quiet -p squidc -- app launch state-counter >/dev/null
output_out="$(wait_for_contains output-start "output=count 0" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=count 0"

run_capture key-select-1 cargo run --quiet -p squidc -- device key SELECT >/dev/null
output_out="$(wait_for_contains output-count-1 "output=count 1" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=count 1"

run_capture key-select-2 cargo run --quiet -p squidc -- device key SELECT >/dev/null
output_out="$(wait_for_contains output-count-2 "output=count 2" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=count 2"

state_out="$(run_capture state-saved cargo run --quiet -p squidc -- device state)"
assert_state_nonempty "${state_out}"

run_capture reset-runtime cargo run --quiet -p squidc -- device reset >/dev/null
run_capture relaunch-state-counter cargo run --quiet -p squidc -- app launch state-counter >/dev/null

output_out="$(wait_for_contains output-reloaded "output=count 2" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=count 2"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr app state SquidScript hardware check passed'
