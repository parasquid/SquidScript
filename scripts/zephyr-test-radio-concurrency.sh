#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
SKIP_FLASH="${SKIP_FLASH:-0}"
REQUIRE_BLE_RECONNECT="${REQUIRE_BLE_RECONNECT:-0}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
DEVICE_SELECTOR="${DEVICE_SELECTOR:-}"
WORK_DIR="${ROOT}/target/hardware-tests/radio-concurrency"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-75}"
BLE_LOG_TIMEOUT_SECONDS="${BLE_LOG_TIMEOUT_SECONDS:-20}"
BLE_SCAN_TIMEOUT_SECONDS="${BLE_SCAN_TIMEOUT_SECONDS:-30}"
BLE_CONNECT_TIMEOUT_SECONDS="${BLE_CONNECT_TIMEOUT_SECONDS:-15}"
HOST_AP_SSID="${HOST_AP_SSID:-SquidHostTest}"
HOST_AP_CONN="${HOST_AP_CONN:-squid-radio-host-ap}"
DEVICE_AP_SSID="${DEVICE_AP_SSID:-SquidScript}"
DEVICE_AP_CONN="${DEVICE_AP_CONN:-squid-radio-device-ap}"
WIFI_LIST_APP="${ROOT}/tests/hardware/zephyr/radio-concurrency/wifi-list/main.squid"
WIFI_STATION_APP="${ROOT}/tests/hardware/zephyr/radio-concurrency/wifi-station/main.squid"
WIFI_AP_APP="${ROOT}/tests/hardware/zephyr/radio-concurrency/wifi-ap/main.squid"
WIFI_STATUS_APP="${ROOT}/tests/hardware/zephyr/radio-concurrency/wifi-status/main.squid"

BLE_ADDR=""
HOST_AP_PASS_FILE=""
DEVICE_AP_ACTIVE=0

usage() {
  cat <<'EOF'
Usage: scripts/zephyr-test-radio-concurrency.sh [--target <id>] [--skip-flash] [--require-ble-reconnect] [--device <name-or-address>] [--host-wifi-iface <iface>]

Runs an opt-in Wi-Fi/BLE concurrency matrix against a Zephyr ESP32-C3 target.
The script may temporarily take over the host Wi-Fi and Bluetooth controllers.
It creates a temporary host AP, connects the target as a station, connects the
host to the target AP, verifies BLE discovery/connectability during Wi-Fi work,
and cleans up host radio state on exit.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET_ID="$2"
      shift 2
      ;;
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    --require-ble-reconnect)
      REQUIRE_BLE_RECONNECT=1
      shift
      ;;
    --device)
      DEVICE_SELECTOR="$2"
      shift 2
      ;;
    --host-wifi-iface)
      HOST_WIFI_IFACE="$2"
      shift 2
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

mkdir -p "${WORK_DIR}"

cleanup_radio_concurrency() {
  set +e
  if [[ "${DEVICE_AP_ACTIVE}" == "1" ]]; then
    timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" cargo run --quiet -p squidc -- device key SELECT \
      >"${WORK_DIR}/cleanup-device-ap-stop.out" 2>&1
  fi
  if [[ -n "${BLE_ADDR}" ]]; then
    timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
      >"${WORK_DIR}/cleanup-ble-disconnect.out" 2>&1
  fi
  if command -v bluetoothctl >/dev/null 2>&1; then
    bluetoothctl scan off >"${WORK_DIR}/cleanup-ble-scan-off.out" 2>&1
  fi
  if command -v nmcli >/dev/null 2>&1; then
    nmcli connection down "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-down.out" 2>&1
    nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-delete.out" 2>&1
    nmcli connection down "${HOST_AP_CONN}" >"${WORK_DIR}/cleanup-host-ap-down.out" 2>&1
    nmcli connection delete "${HOST_AP_CONN}" >"${WORK_DIR}/cleanup-host-ap-delete.out" 2>&1
    if [[ -n "${HOST_WIFI_IFACE}" ]]; then
      nmcli device connect "${HOST_WIFI_IFACE}" >"${WORK_DIR}/cleanup-host-wifi-connect.out" 2>&1
    fi
  fi
  if [[ -n "${HOST_AP_PASS_FILE}" ]]; then
    rm -f "${HOST_AP_PASS_FILE}"
  fi
}
trap cleanup_radio_concurrency EXIT

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
    printf 'Expected %s to contain only recognized non-radio diagnostics\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_no_raw_network_identifiers() {
  local file="$1"
  if grep -Eq '([[:xdigit:]]{2}:){5}[[:xdigit:]]{2}|([0-9]{1,3}\.){3}[0-9]{1,3}' "${file}"; then
    printf 'Expected %s not to contain raw MAC or local IP identifiers\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
  if [[ -n "${HOST_AP_PASS_FILE}" ]] && [[ -s "${HOST_AP_PASS_FILE}" ]]; then
    local password
    password="$(cat "${HOST_AP_PASS_FILE}")"
    if grep -Fq "${password}" "${file}"; then
      printf 'Expected %s not to contain raw Wi-Fi credentials\n' "${file}" >&2
      printf '%s\n' "--- ${file} ---" >&2
      sed -n '1,200p' "${file}" >&2
      exit 1
    fi
  fi
  if grep -Fq "${HOST_AP_SSID}" "${file}" || grep -Fq "${DEVICE_AP_SSID}" "${file}"; then
    printf 'Expected %s not to contain raw test SSIDs\n' "${file}" >&2
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
    sleep 0.2
  done
  printf 'Timed out waiting for %s in %s\n' "${expected}" "${command_name}" >&2
  capture_device_diagnostics "${label}-timeout"
  exit 1
}

python_json_field() {
  local file="$1"
  local field="$2"
  python3 - "$file" "$field" <<'PY'
import json
import sys
data = json.load(open(sys.argv[1], encoding="utf-8"))["data"]
value = data
for part in sys.argv[2].split("."):
    value = value[part]
print(value)
PY
}

detect_host_wifi_iface() {
  if [[ -n "${HOST_WIFI_IFACE}" ]]; then
    return 0
  fi
  HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status |
    awk -F: '$2 == "wifi" && $3 == "connected" { print $1; exit }')"
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status |
      awk -F: '$2 == "wifi" { print $1; exit }')"
  fi
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    printf '%s\n' 'No host Wi-Fi interface found for radio concurrency test' >&2
    exit 1
  fi
}

generate_host_ap_password() {
  HOST_AP_PASS_FILE="$(mktemp "${WORK_DIR}/host-ap-pass.XXXXXX")"
  local password
  password="$(python3 - <<'PY'
import secrets
import string
alphabet = string.ascii_letters + string.digits
print("Sq" + "".join(secrets.choice(alphabet) for _ in range(14)) + "9")
PY
)"
  printf '%s' "${password}" >"${HOST_AP_PASS_FILE}"
}

start_host_ap() {
  detect_host_wifi_iface
  generate_host_ap_password
  nmcli connection delete "${HOST_AP_CONN}" >"${WORK_DIR}/host-ap-delete-existing.out" 2>&1 || true
  nmcli connection add type wifi ifname "${HOST_WIFI_IFACE}" con-name "${HOST_AP_CONN}" ssid "${HOST_AP_SSID}" \
    >"${WORK_DIR}/host-ap-add.out" 2>&1
  nmcli connection modify "${HOST_AP_CONN}" \
    802-11-wireless.mode ap \
    802-11-wireless.band bg \
    ipv4.method shared \
    wifi-sec.key-mgmt wpa-psk \
    wifi-sec.psk "$(cat "${HOST_AP_PASS_FILE}")" \
    >"${WORK_DIR}/host-ap-configure.out" 2>&1
  nmcli connection up "${HOST_AP_CONN}" >"${WORK_DIR}/host-ap-up.out" 2>&1
  printf '%s\n' 'OK host Wi-Fi AP started for station concurrency check'
}

stop_host_ap() {
  nmcli connection down "${HOST_AP_CONN}" >"${WORK_DIR}/host-ap-down.out" 2>&1 || true
  nmcli connection delete "${HOST_AP_CONN}" >"${WORK_DIR}/host-ap-delete.out" 2>&1 || true
  rm -f "${HOST_AP_PASS_FILE}"
  HOST_AP_PASS_FILE=""
  printf '%s\n' 'OK host Wi-Fi AP stopped after station concurrency check'
}

connect_host_to_device_ap() {
  detect_host_wifi_iface
  nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-delete-existing.out" 2>&1 || true
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local connected=0
  while (( SECONDS < deadline )); do
    nmcli device wifi rescan ifname "${HOST_WIFI_IFACE}" >"${WORK_DIR}/device-ap-rescan.out" 2>&1 || true
    sleep 1
    if nmcli device wifi connect "${DEVICE_AP_SSID}" ifname "${HOST_WIFI_IFACE}" name "${DEVICE_AP_CONN}" \
      >"${WORK_DIR}/device-ap-connect.out" 2>&1; then
      connected=1
      break
    fi
    sleep 1
  done
  if [[ "${connected}" != "1" ]]; then
    printf '%s\n' 'Expected host Wi-Fi to associate to target AP within timeout' >&2
    exit 1
  fi
  if ! nmcli -t -f GENERAL.STATE device show "${HOST_WIFI_IFACE}" >"${WORK_DIR}/device-ap-state.out" 2>&1; then
    printf '%s\n' 'Expected host Wi-Fi state query to succeed after device AP connect' >&2
    exit 1
  fi
  printf '%s\n' 'OK host Wi-Fi associated to device AP during BLE connection'
}

assert_device_ap_dhcp_lease() {
  detect_host_wifi_iface
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    nmcli -t -f IP4.ADDRESS device show "${HOST_WIFI_IFACE}" \
      >"${WORK_DIR}/device-ap-ipv4.raw" 2>&1 || true
    if python3 - "${WORK_DIR}/device-ap-ipv4.raw" <<'PY'
import re
import sys

with open(sys.argv[1], encoding="utf-8", errors="replace") as handle:
    text = handle.read()

for match in re.finditer(r"192\.168\.4\.(\d+)/(\d+)", text):
    host = int(match.group(1))
    prefix = int(match.group(2))
    if 2 <= host <= 254 and prefix == 24:
        raise SystemExit(0)

raise SystemExit(1)
PY
    then
      printf '%s\n' 'OK host Wi-Fi received target AP DHCP lease'
      return 0
    fi
    sleep 0.5
  done
  printf '%s\n' 'Expected host Wi-Fi to receive a target AP DHCP lease within timeout' >&2
  exit 1
}

disconnect_host_from_device_ap() {
  nmcli connection down "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-down.out" 2>&1 || true
  nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-delete.out" 2>&1 || true
}

ble_name_prefix() {
  python3 - "$1" <<'PY'
import sys
name = sys.argv[1]
print(name.encode("utf-8")[:29].decode("utf-8", "ignore"))
PY
}

capture_ble_advertising_log() {
  local target_name="$1"
  local port
  port="$(resolve_esp_serial_port)"
  local monitor_log="${WORK_DIR}/ble-advertising.out"
  local monitor_cmd=(cargo run --quiet -p squidc -- target monitor --target "${TARGET_ID}" --port "${port}")
  local monitor_shell_command
  monitor_shell_command="$(printf '%q ' "${monitor_cmd[@]}")"
  set +e
  if command -v script >/dev/null 2>&1; then
    (
      cd "${ROOT}"
      timeout "${BLE_LOG_TIMEOUT_SECONDS}s" script -q -e -c "${monitor_shell_command}" /dev/null
    ) >"${monitor_log}" 2>&1
  else
    (
      cd "${ROOT}"
      timeout "${BLE_LOG_TIMEOUT_SECONDS}s" "${monitor_cmd[@]}"
    ) >"${monitor_log}" 2>&1
  fi
  local monitor_status=$?
  set -e
  assert_file_contains "${monitor_log}" "BLE advertising started: ${target_name}"
  if [[ "${monitor_status}" != "0" && "${monitor_status}" != "124" ]]; then
    printf 'Target monitor exited with status %s after BLE advertising capture\n' "${monitor_status}" >&2
    exit "${monitor_status}"
  fi
  printf '%s\n' 'OK BLE serial advertising observed'
}

discover_ble_device() {
  local target_name="$1"
  local target_prefix
  local selector="${DEVICE_SELECTOR:-$1}"
  target_prefix="$(ble_name_prefix "${target_name}")"
  if ! command -v bluetoothctl >/dev/null 2>&1; then
    printf '%s\n' 'bluetoothctl is required for radio concurrency testing' >&2
    exit 1
  fi
  if ! bluetoothctl show >"${WORK_DIR}/ble-controller.out" 2>&1; then
    printf '%s\n' 'No usable host Bluetooth controller found for radio concurrency testing' >&2
    exit 1
  fi
  local scan_out="${WORK_DIR}/ble-scan.raw"
  set +e
  timeout "$((BLE_SCAN_TIMEOUT_SECONDS + 3))s" bluetoothctl --timeout "${BLE_SCAN_TIMEOUT_SECONDS}" scan on >"${scan_out}" 2>&1
  local scan_status=$?
  set -e
  if [[ "${scan_status}" != "0" && "${scan_status}" != "124" ]]; then
    printf 'Host Bluetooth scan exited with status %s\n' "${scan_status}" >&2
    exit "${scan_status}"
  fi
  bluetoothctl scan off >"${WORK_DIR}/ble-scan-off.out" 2>&1 || true
  BLE_ADDR="$(python3 - "${scan_out}" "${target_name}" "${target_prefix}" "${selector}" <<'PY'
import re
import sys
path, full_name, prefix, selector = sys.argv[1:5]
ansi = re.compile(r"\x1b\[[0-9;]*m")
device_line = re.compile(r"Device\s+([0-9A-Fa-f:]{17})\s+(.+)$")
name_change = re.compile(r"Device\s+([0-9A-Fa-f:]{17})\s+Name:\s+(.+)$")
with open(path, encoding="utf-8", errors="replace") as handle:
    for line in handle:
        line = ansi.sub("", line)
        if "[DEL]" in line:
            continue
        match = name_change.search(line) or device_line.search(line)
        if not match:
            continue
        address = match.group(1).strip()
        name = match.group(2).strip()
        if selector.lower() == address.lower() or selector in name or full_name in name or prefix in name:
            print(address)
            break
PY
)"
  if [[ -z "${BLE_ADDR}" ]]; then
    printf '%s\n' 'Expected host Bluetooth scan to discover target BLE name or legacy truncated prefix' >&2
    exit 1
  fi
  printf '%s\n' 'OK BLE host discovery matched target advertising name'
}

assert_ble_connected() {
  timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl info "${BLE_ADDR}" \
    >"${WORK_DIR}/ble-info.out" 2>&1
  assert_file_contains "${WORK_DIR}/ble-info.out" "Connected: yes"
}

ble_is_connected() {
  timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl info "${BLE_ADDR}" \
    >"${WORK_DIR}/ble-info-check.out" 2>&1 &&
    grep -Fq "Connected: yes" "${WORK_DIR}/ble-info-check.out"
}

connect_ble_device() {
  local deadline=$((SECONDS + BLE_CONNECT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl connect "${BLE_ADDR}" \
      >"${WORK_DIR}/ble-connect-attempt.out" 2>&1 || true
    if ble_is_connected; then
      printf '%s\n' 'OK BLE host connection established'
      return 0
    fi
    timeout 5s bluetoothctl --timeout 4 scan on >"${WORK_DIR}/ble-connect-refresh.out" 2>&1 || true
    bluetoothctl scan off >"${WORK_DIR}/ble-connect-refresh-off.out" 2>&1 || true
    sleep 1
  done
  printf '%s\n' 'Expected host Bluetooth to connect to target within timeout' >&2
  exit 1
}

ensure_ble_connected() {
  if [[ -z "${BLE_ADDR}" ]]; then
    return 1
  fi
  if ble_is_connected; then
    return 0
  fi
  connect_ble_device
}

disconnect_ble_device() {
  timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
    >"${WORK_DIR}/ble-disconnect.out" 2>&1 || true
  printf '%s\n' 'OK BLE host disconnect requested'
}

install_and_launch_app() {
  local label="$1"
  local path="$2"
  local app_id="$3"
  run_capture "install-${label}" cargo run --quiet -p squidc -- app install "${path}" >/dev/null
  run_capture "launch-${label}" cargo run --quiet -p squidc -- app launch "${app_id}" >/dev/null
}

check_device_errors() {
  local errors_out
  errors_out="$(run_capture "errors-${1}" cargo run --quiet -p squidc -- device errors)"
  assert_no_unexpected_device_errors "${errors_out}"
}

launch_fallback_ble_installer() {
  run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
  run_capture launch-fallback-main cargo run --quiet -p squidc -- app launch main >/dev/null
  local ready_out
  ready_out="$(wait_for_contains output-ble-installer "output=ble installer ready" \
    "device output" cargo run --quiet -p squidc -- device output)"
  assert_no_raw_network_identifiers "${ready_out}"
  printf '%s\n' 'OK fallback BLE installer launched'
}

target_out="${WORK_DIR}/target-inspect.json"
cargo run --quiet -p squidc -- --json target inspect --target "${TARGET_ID}" >"${target_out}"
target_name="$(python_json_field "${target_out}" "name")"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}"
else
  cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
fi

launch_fallback_ble_installer
discover_ble_device "${target_name}"
connect_ble_device

install_and_launch_app wifi-list "${WIFI_LIST_APP}" radio-wifi-list
list_out="$(wait_for_contains output-wifi-list "output=radio wifi list true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${list_out}"
assert_ble_connected
check_device_errors wifi-list
printf '%s\n' 'OK BLE stayed connected during Wi-Fi scan/list'

run_capture reset-before-wifi-ap cargo run --quiet -p squidc -- device reset >/dev/null
ensure_ble_connected
install_and_launch_app wifi-ap "${WIFI_AP_APP}" radio-wifi-ap
ap_start_out="$(wait_for_contains output-wifi-ap-start "output=radio wifi ap start true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${ap_start_out}" "output=radio wifi ap ip null"
assert_no_raw_network_identifiers "${ap_start_out}"
DEVICE_AP_ACTIVE=1
connect_host_to_device_ap
assert_device_ap_dhcp_lease
assert_ble_connected
disconnect_host_from_device_ap
run_capture ap-stop-key cargo run --quiet -p squidc -- device key SELECT >/dev/null
ap_stop_out="$(wait_for_contains output-wifi-ap-stop "output=radio wifi ap stop true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${ap_stop_out}"
DEVICE_AP_ACTIVE=0
check_device_errors wifi-ap
printf '%s\n' 'OK BLE stayed connected during Wi-Fi AP client association'

run_capture reset-before-wifi-station cargo run --quiet -p squidc -- device reset >/dev/null
ensure_ble_connected
start_host_ap
export SQUID_WIFI_STATION_SSID="${HOST_AP_SSID}"
export SQUID_WIFI_STATION_PASSWORD
SQUID_WIFI_STATION_PASSWORD="$(cat "${HOST_AP_PASS_FILE}")"
run_capture wifi-profile cargo run --quiet -p squidc -- device wifi-profile dev \
  --ssid-env SQUID_WIFI_STATION_SSID \
  --password-env SQUID_WIFI_STATION_PASSWORD >/dev/null
assert_no_raw_network_identifiers "${WORK_DIR}/wifi-profile.out"
install_and_launch_app wifi-station "${WIFI_STATION_APP}" radio-wifi-station
station_out="$(wait_for_contains output-wifi-station "output=radio wifi station dev true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${station_out}" "output=radio wifi connect true null"
assert_no_raw_network_identifiers "${station_out}"
assert_ble_connected
run_capture station-disconnect-key cargo run --quiet -p squidc -- device key SELECT >/dev/null
station_disconnect_out="$(wait_for_contains output-wifi-disconnect "output=radio wifi disconnect true" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${station_disconnect_out}"
station_disconnected_out="$(wait_for_contains output-wifi-disconnected "output=radio wifi disconnected dev false" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${station_disconnected_out}"
check_device_errors wifi-station
stop_host_ap
printf '%s\n' 'OK BLE stayed connected during Wi-Fi station connect/disconnect'

run_capture reset-before-wifi-status cargo run --quiet -p squidc -- device reset >/dev/null
ensure_ble_connected
install_and_launch_app wifi-status "${WIFI_STATUS_APP}" radio-wifi-status
status_out="$(wait_for_contains output-wifi-status "output=radio wifi status" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_no_raw_network_identifiers "${status_out}"
assert_ble_connected
check_device_errors recovery
disconnect_ble_device
if [[ "${REQUIRE_BLE_RECONNECT}" == "1" ]]; then
  discover_ble_device "${target_name}"
  connect_ble_device
  disconnect_ble_device
fi

printf '%s\n' 'OK Zephyr Wi-Fi/BLE radio concurrency hardware check passed'
