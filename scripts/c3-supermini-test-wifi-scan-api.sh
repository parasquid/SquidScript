#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/wifi-scan"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
WIFI_SCAN_APP="${ROOT}/tests/hardware/c3-supermini/wifi-scan-summary/main.squid"
REQUIRE_REAL_WIFI=0

mkdir -p "${WORK_DIR}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-wifi-scan-api.sh [--require-real-wifi]

Runs the Zephyr Wi-Fi scan SquidScript hardware check. With
--require-real-wifi, the output must prove a real Zephyr Wi-Fi scan completed.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --require-real-wifi)
      REQUIRE_REAL_WIFI=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done


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

assert_no_raw_network_identifiers() {
  local file="$1"
  if grep -Eq '([[:xdigit:]]{2}:){5}[[:xdigit:]]{2}|([0-9]{1,3}\.){3}[0-9]{1,3}' "${file}"; then
    printf 'Expected %s not to contain raw BSSID, MAC, or local IP identifiers\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

capture_timeout_diagnostics() {
  local label="$1"

  timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
    cargo run --quiet -p squidc -- device resources \
    >"${WORK_DIR}/${label}-resources.out" 2>&1 || true
  timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
    cargo run --quiet -p squidc -- device errors \
    >"${WORK_DIR}/${label}-errors.out" 2>&1 || true
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
  capture_timeout_diagnostics "${label}-timeout"
  printf 'timeout diagnostics: %s %s\n' \
    "${WORK_DIR}/${label}-timeout-resources.out" \
    "${WORK_DIR}/${label}-timeout-errors.out" >&2
  exit 1
}

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-wifi-scan cargo run --quiet -p squidc -- app install "${WIFI_SCAN_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=wifi-scan-summary"

run_capture launch-wifi-scan cargo run --quiet -p squidc -- app launch wifi-scan-summary >/dev/null

output_out="$(wait_for_contains output "output=wifi scan" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${output_out}"
if [[ "${REQUIRE_REAL_WIFI}" == "1" ]]; then
  if ! grep -Fq "wifi scan true" "${output_out}"; then
    printf 'Expected %s to contain a successful real Zephyr Wi-Fi scan\n' "${output_out}" >&2
    printf '%s\n' "--- ${output_out} ---" >&2
    sed -n '1,200p' "${output_out}" >&2
    exit 1
  fi
fi

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr Wi-Fi scan SquidScript hardware check passed'
