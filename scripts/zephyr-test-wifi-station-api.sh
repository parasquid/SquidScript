#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/wifi-station"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
WIFI_STATION_APP="${ROOT}/tests/hardware/c3-supermini/wifi-station-summary/main.squid"

station_ssid="$(printenv SQUID_WIFI_STATION_SSID || true)"
station_password="$(printenv SQUID_WIFI_STATION_PASSWORD || true)"

if [[ -z "${station_ssid}" || -z "${station_password}" ]]; then
  printf '%s\n' \
    'OK Zephyr Wi-Fi station check credentials not provided; skipping explicit station test'
  exit 0
fi

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

assert_no_unexpected_device_errors() {
  local file="$1"
  if grep -Evq '^error=display=unavailable code=-19( \(ENODEV\))?$' "${file}"; then
    printf 'Expected %s to contain only recognized non-Wi-Fi diagnostics\n' "${file}" >&2
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
  if grep -Fq "${station_ssid}" "${file}" ||
    grep -Fq "${station_password}" "${file}"; then
    printf 'Expected %s not to contain raw Wi-Fi credentials\n' "${file}" >&2
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
run_capture wifi-profile cargo run --quiet -p squidc -- device wifi-profile dev \
  --ssid-env SQUID_WIFI_STATION_SSID \
  --password-env SQUID_WIFI_STATION_PASSWORD >/dev/null
assert_no_raw_network_identifiers "${WORK_DIR}/wifi-profile.out"

run_capture install-wifi-station cargo run --quiet -p squidc -- app install "${WIFI_STATION_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=wifi-station-summary"

run_capture launch-wifi-station cargo run --quiet -p squidc -- app launch wifi-station-summary >/dev/null

output_out="$(wait_for_contains output "output=wifi station dev true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=wifi connect true null"
if grep -Fq "unsupported" "${output_out}"; then
  printf 'Expected %s not to contain unsupported Wi-Fi station results\n' "${output_out}" >&2
  printf '%s\n' "--- ${output_out} ---" >&2
  sed -n '1,200p' "${output_out}" >&2
  exit 1
fi
assert_no_raw_network_identifiers "${output_out}"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_no_unexpected_device_errors "${errors_out}"

printf '%s\n' 'OK Zephyr Wi-Fi station SquidScript hardware check passed'
