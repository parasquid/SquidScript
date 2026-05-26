#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/unsupported-inline-gpio-binding"
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

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-unsupported-inline-gpio cargo run --quiet -p squidc -- app install "${UNSUPPORTED_INLINE_GPIO_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=unsupported-inline-gpio-binding"

assert_command_fails_contains launch-unsupported-inline-gpio "unsupported (-95)" \
  cargo run --quiet -p squidc -- app launch unsupported-inline-gpio-binding

output_out="$(run_capture output cargo run --quiet -p squidc -- device output)"
assert_file_empty_command "${output_out}"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr unsupported inline GPIO binding hardware check passed'
