#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/foreground-memory"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
MEMORY_APP="${ROOT}/tests/hardware/c3-supermini/foreground-memory/main.squid"

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

assert_file_count_at_least() {
  local file="$1"
  local expected="$2"
  local minimum="$3"
  local count
  count="$(grep -F "${expected}" "${file}" | wc -l)"
  if (( count < minimum )); then
    printf 'Expected %s to contain at least %s occurrence(s) of: %s\n' "${file}" "${minimum}" "${expected}" >&2
    printf 'Found %s occurrence(s)\n' "${count}" >&2
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
    timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@" >"${out}" 2>&1
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
run_capture install-foreground-memory cargo run --quiet -p squidc -- app install "${MEMORY_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=foreground-memory"

run_capture launch-foreground-memory cargo run --quiet -p squidc -- app launch foreground-memory >/dev/null
output_out="$(wait_for_contains output-start "output=memory start 1" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=memory start 1"

run_capture key-select-1 cargo run --quiet -p squidc -- device key SELECT >/dev/null
output_out="$(wait_for_contains output-select-2 "output=memory select 2" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=memory select 2"

run_capture key-select-2 cargo run --quiet -p squidc -- device key SELECT >/dev/null
output_out="$(wait_for_contains output-select-3 "output=memory select 3" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=memory select 3"

run_capture relaunch-foreground-memory cargo run --quiet -p squidc -- app launch foreground-memory >/dev/null
output_out="$(wait_for_contains output-relaunch "output=memory start 1" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_count_at_least "${output_out}" "output=memory start 1" 2

run_capture key-select-after-relaunch cargo run --quiet -p squidc -- device key SELECT >/dev/null
output_out="$(wait_for_contains output-select-after-relaunch "output=memory select 2" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_count_at_least "${output_out}" "output=memory select 2" 2

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr foreground in-memory state hardware check passed'
