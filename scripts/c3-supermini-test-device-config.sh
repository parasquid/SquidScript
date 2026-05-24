#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/device-config"
DEVICE_CONFIG_APP="${ROOT}/tests/hardware/c3-supermini/device-config-summary"
DEVICE_CONFIG_PACKAGE="${WORK_DIR}/device-config-summary.squid.zip"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware device config: %s\n' "$*" >&2
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

  for _ in $(seq 1 80); do
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

run_capture package-device-config cargo run --quiet -p squidc -- package "${DEVICE_CONFIG_APP}" --out "${DEVICE_CONFIG_PACKAGE}" >/dev/null
run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-device-config cargo run --quiet -p squidc -- app install "${DEVICE_CONFIG_PACKAGE}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=device-config-summary"

run_capture launch-device-config cargo run --quiet -p squidc -- app launch device-config-summary >/dev/null

output_out="$(wait_for_contains output-device-save "output=device save true null null" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=device load true null null"
assert_file_contains "${output_out}" "output=device set true null null"
assert_file_contains "${output_out}" "output=device rebind true null null"
assert_file_contains "${output_out}" "output=device save true null null"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr device config SquidScript hardware check passed'
