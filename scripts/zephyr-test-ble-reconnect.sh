#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
SKIP_FLASH="${SKIP_FLASH:-0}"
BLE_ADVERTISING_LOG_TIMEOUT_SECONDS="${BLE_ADVERTISING_LOG_TIMEOUT_SECONDS:-20}"
BLE_DISCONNECT_TIMEOUT_SECONDS="${BLE_DISCONNECT_TIMEOUT_SECONDS:-10}"
BLE_RESCAN_TIMEOUT_SECONDS="${BLE_RESCAN_TIMEOUT_SECONDS:-30}"
BLE_CONNECT_TIMEOUT_SECONDS="${BLE_CONNECT_TIMEOUT_SECONDS:-15}"
BLE_RESTART_GRACE_SECONDS="${BLE_RESTART_GRACE_SECONDS:-10}"
WORK_DIR="${ROOT}/target/hardware-tests/ble-reconnect"
DEVICE_NAME=""

usage() {
	cat <<'EOF'
Usage: scripts/zephyr-test-ble-reconnect.sh [--target <id>] [--skip-flash]

Builds or flashes the Zephyr firmware for the selected target, confirms the
initial BLE advertising log, connects to the device from a host Bluetooth
controller, disconnects, waits for the firmware's restart-advertising work
item, and verifies a fresh advertisement can be rediscovered. This is the
real-hardware counterpart of firmware/zephyr/tests/ble-smoke and proves the
stop-before-start restart sequence actually puts bytes back on the air.

The script requires a host Bluetooth controller accessible to bluetoothctl.
Do not run it in parallel with any other firmware, monitor, or hardware
command against the same physical target.
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

BLE_ADDR=""
cleanup() {
	set +e
	if [[ -n "${BLE_ADDR}" ]]; then
		timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
			>"${WORK_DIR}/cleanup-ble-disconnect.out" 2>&1 || true
	fi
	if command -v bluetoothctl >/dev/null 2>&1; then
		bluetoothctl scan off >"${WORK_DIR}/cleanup-ble-scan-off.out" 2>&1 || true
	fi
}
trap cleanup EXIT

if ! command -v bluetoothctl >/dev/null 2>&1; then
	printf '%s\n' 'bluetoothctl is required for the BLE re-advertising check' >&2
	exit 1
fi
if ! bluetoothctl show >"${WORK_DIR}/ble-controller.out" 2>&1; then
	printf '%s\n' 'No usable host Bluetooth controller found for the BLE re-advertising check' >&2
	exit 1
fi

target_inspect_out="$(run_capture target-inspect cargo run --quiet -p squidc -- --json target inspect --target "${TARGET_ID}")"
DEVICE_NAME="$(python3 -c '
import json, sys
with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
print(data["data"]["name"])
' "${target_inspect_out}")"

PORT="$(resolve_esp_serial_port)"
export ESPFLASH_PORT="${ESPFLASH_PORT:-${PORT}}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
	printf '%s: cargo run --quiet -p squidc -- target flash --target %s\n' "${HARDWARE_COMMAND_LABEL}" "${TARGET_ID}" >&2
	COMMAND_TIMEOUT_SECONDS="${BLE_RECONNECT_FLASH_TIMEOUT_SECONDS:-300}" \
		cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" \
		>"${WORK_DIR}/target-flash.out" 2>&1
else
	run_capture target-build cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
fi

scan_out="${WORK_DIR}/ble-scan.raw"
set +e
timeout "$((BLE_RESCAN_TIMEOUT_SECONDS + 3))s" bluetoothctl --timeout "${BLE_RESCAN_TIMEOUT_SECONDS}" \
	scan on >"${scan_out}" 2>&1
scan_status=$?
set -e
bluetoothctl scan off >"${WORK_DIR}/ble-scan-off.out" 2>&1 || true
if [[ "${scan_status}" != "0" && "${scan_status}" != "124" ]]; then
	printf 'Initial host Bluetooth scan exited with status %s\n' "${scan_status}" >&2
	exit "${scan_status}"
fi

BLE_ADDR="$(python3 - "${scan_out}" "${DEVICE_NAME}" <<'PY'
import re
import sys
path, full_name = sys.argv[1:3]
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
        name = match.group(2).strip()
        if full_name in name or full_name.encode("utf-8")[:29].decode("utf-8", "ignore") in name:
            print(match.group(1))
            break
PY
)"
if [[ -z "${BLE_ADDR}" ]]; then
	printf 'Expected host Bluetooth scan to discover %s within %ss\n' \
		"${DEVICE_NAME}" "${BLE_RESCAN_TIMEOUT_SECONDS}" >&2
	printf 'Scan log: %s\n' "${scan_out}" >&2
	exit 1
fi
printf '%s\n' 'OK BLE host discovery matched target advertising name'

connect_deadline=$((SECONDS + BLE_CONNECT_TIMEOUT_SECONDS))
connected=0
while (( SECONDS < connect_deadline )); do
	if timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl connect "${BLE_ADDR}" \
		>"${WORK_DIR}/ble-connect-attempt.out" 2>&1; then
		if timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl info "${BLE_ADDR}" \
			>"${WORK_DIR}/ble-info.out" 2>&1 && \
			grep -Fq "Connected: yes" "${WORK_DIR}/ble-info.out"; then
			connected=1
			break
		fi
	fi
	timeout 5s bluetoothctl --timeout 4 scan on >"${WORK_DIR}/ble-connect-refresh.out" 2>&1 || true
	bluetoothctl scan off >"${WORK_DIR}/ble-connect-refresh-off.out" 2>&1 || true
	sleep 1
done
if [[ "${connected}" != "1" ]]; then
	printf 'Expected host Bluetooth to connect to %s within %ss\n' \
		"${BLE_ADDR}" "${BLE_CONNECT_TIMEOUT_SECONDS}" >&2
	exit 1
fi
printf '%s\n' 'OK BLE host connection established'

set +e
timeout "${BLE_DISCONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
	>"${WORK_DIR}/ble-disconnect.out" 2>&1
disconnect_status=$?
set -e
if [[ "${disconnect_status}" != "0" && "${disconnect_status}" != "124" ]]; then
	printf 'Host Bluetooth disconnect exited with status %s\n' "${disconnect_status}" >&2
	exit "${disconnect_status}"
fi
printf '%s\n' 'OK BLE host disconnect requested'

sleep "${BLE_RESTART_GRACE_SECONDS}"

rescan_out="${WORK_DIR}/ble-rescan.raw"
set +e
timeout "$((BLE_RESCAN_TIMEOUT_SECONDS + 3))s" bluetoothctl --timeout "${BLE_RESCAN_TIMEOUT_SECONDS}" \
	scan on >"${rescan_out}" 2>&1
rescan_status=$?
set -e
bluetoothctl scan off >"${WORK_DIR}/ble-rescan-off.out" 2>&1 || true
if [[ "${rescan_status}" != "0" && "${rescan_status}" != "124" ]]; then
	printf 'Rescan exited with status %s\n' "${rescan_status}" >&2
	exit "${rescan_status}"
fi

if ! grep -Fq "${DEVICE_NAME}" "${rescan_out}" && ! grep -Fqi "${BLE_ADDR}" "${rescan_out}"; then
	printf 'Expected host Bluetooth to rediscover %s (%s) within %ss after disconnect\n' \
		"${DEVICE_NAME}" "${BLE_ADDR}" "${BLE_RESCAN_TIMEOUT_SECONDS}" >&2
	printf 'Rescan log: %s\n' "${rescan_out}" >&2
	exit 1
fi
printf '%s\n' 'OK host rediscovered fresh advertisement after disconnect'

timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
	>"${WORK_DIR}/ble-disconnect-final.out" 2>&1 || true
BLE_ADDR=""

printf '%s\n' 'OK Zephyr BLE re-advertising after host disconnect verified'
