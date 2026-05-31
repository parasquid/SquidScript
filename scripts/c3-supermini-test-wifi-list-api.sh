#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/wifi-list"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
WIFI_LIST_APP="${ROOT}/tests/hardware/c3-supermini/wifi-list-summary/main.squid"
REQUIRE_REAL_WIFI=0

mkdir -p "${WORK_DIR}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-wifi-list-api.sh [--require-real-wifi]

Runs the Zephyr Wi-Fi list SquidScript hardware check. The app iterates
service.wifi.scan().networks and prints redacted structural AP fields only:
SSID length, channel, RSSI, auth, and hidden flag.
With --require-real-wifi, the output must prove a real Zephyr Wi-Fi scan
completed and returned at least one redacted AP row.
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

assert_no_null_auth_rows() {
  local file="$1"
  if awk '$1 == "output=wifi" && $2 == "ap" && $6 == "null" { found = 1 } END { exit found ? 0 : 1 }' "${file}"; then
    printf 'Expected %s not to contain redacted AP rows with null auth\n' "${file}" >&2
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
run_capture install-wifi-list cargo run --quiet -p squidc -- app install "${WIFI_LIST_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=wifi-list-summary"

run_capture launch-wifi-list cargo run --quiet -p squidc -- app launch wifi-list-summary >/dev/null

output_out="$(wait_for_contains output "output=wifi list" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${output_out}"
assert_no_null_auth_rows "${output_out}"
if [[ "${REQUIRE_REAL_WIFI}" == "1" ]]; then
  if grep -Fq "wifi list true" "${output_out}"; then
    assert_file_contains "${output_out}" "wifi ap"
  else
    printf 'Expected %s to contain a successful real Zephyr Wi-Fi scan list\n' "${output_out}" >&2
    printf '%s\n' "--- ${output_out} ---" >&2
    sed -n '1,200p' "${output_out}" >&2
    exit 1
  fi
fi

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr Wi-Fi list SquidScript hardware check passed'
