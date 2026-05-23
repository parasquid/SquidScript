#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/app-lifecycle"
MAIN_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/main.squid"
BREAK_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/break-reminder.squid"
READER_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/reader-clock.squid"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware app lifecycle: %s\n' "$*" >&2
  "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '--- %s ---\n' "${file}" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '--- %s ---\n' "${file}" >&2
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
  printf '--- %s ---\n' "${out}" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-main cargo run --quiet -p squidc -- app install "${MAIN_APP}" >/dev/null
run_capture install-break cargo run --quiet -p squidc -- app install "${BREAK_APP}" >/dev/null
run_capture install-reader cargo run --quiet -p squidc -- app install "${READER_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=main"
assert_file_contains "${apps_out}" "app=break-reminder"
assert_file_contains "${apps_out}" "app=reader-clock"

run_capture launch-main cargo run --quiet -p squidc -- app launch main >/dev/null

lifecycle_out="$(wait_for_contains lifecycle-reader "lifecycle=active=reader-clock" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"
assert_file_contains "${lifecycle_out}" "lifecycle=armed_stack[0]=break-reminder timer.break"

output_out="$(wait_for_contains output-reader "output=reader start" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=main start"
assert_file_contains "${output_out}" "output=break armed"

lifecycle_out="$(wait_for_contains lifecycle-break "lifecycle=active=break-reminder" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[1]=reader-clock"

output_out="$(wait_for_contains output-break "output=break fired" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=reader start"

run_capture exit-break cargo run --quiet -p squidc -- device key SELECT >/dev/null

lifecycle_out="$(wait_for_contains lifecycle-return "lifecycle=active=reader-clock" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"

output_out="$(wait_for_contains output-return "output=break exit" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=reader start"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr app lifecycle SquidScript hardware check passed'
