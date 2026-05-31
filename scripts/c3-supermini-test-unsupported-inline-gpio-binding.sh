#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/unsupported-inline-gpio-binding"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-20}"
UNSUPPORTED_INLINE_GPIO_APP="${ROOT}/tests/hardware/c3-supermini/unsupported-inline-gpio-binding/main.squid"

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
run_capture install-unsupported-inline-gpio cargo run --quiet -p squidc -- app install "${UNSUPPORTED_INLINE_GPIO_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=unsupported-inline-gpio-binding"

run_capture launch-unsupported-inline-gpio \
  cargo run --quiet -p squidc -- app launch unsupported-inline-gpio-binding >/dev/null

output_out="$(run_capture output cargo run --quiet -p squidc -- device output)"
assert_file_empty_command "${output_out}"

errors_out="$(wait_for_contains errors "runtime=host_error" \
  "device errors" cargo run --quiet -p squidc -- device errors)"
assert_file_contains "${errors_out}" "runtime=host_error"

printf '%s\n' 'OK Zephyr unsupported inline GPIO binding hardware check passed'
