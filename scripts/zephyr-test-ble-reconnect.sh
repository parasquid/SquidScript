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
DEVICE_SELECTOR=""
PORT=""
MONITOR_PID=""
MONITOR_LOG="${WORK_DIR}/serial-monitor.log"

usage() {
	cat <<'EOF'
Usage: scripts/zephyr-test-ble-reconnect.sh [--target <id>] [--device <name-or-address>] [--skip-flash]

Builds or flashes the Zephyr firmware for the selected target, launches the
fallback main app to start its BLE file-transfer profile, connects to the
device from a host Bluetooth controller, disconnects, watches the firmware's
restart-advertising log sequence, and verifies a fresh advertisement can be
rediscovered. This is the real-hardware counterpart of
firmware/zephyr/tests/ble-smoke and proves the stop-before-start restart
sequence actually puts bytes back on the air.

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
		--device)
			DEVICE_SELECTOR="$2"
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

BLE_ADDR=""
cleanup() {
	set +e
	if [[ -n "${MONITOR_PID}" ]]; then
		kill "${MONITOR_PID}" >/dev/null 2>&1 || true
		wait "${MONITOR_PID}" >/dev/null 2>&1 || true
	fi
	if [[ -n "${BLE_ADDR}" ]]; then
		timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
			>"${WORK_DIR}/cleanup-ble-disconnect.out" 2>&1 || true
	fi
	if command -v bluetoothctl >/dev/null 2>&1; then
		bluetoothctl scan off >"${WORK_DIR}/cleanup-ble-scan-off.out" 2>&1 || true
	fi
}
trap cleanup EXIT

start_serial_monitor() {
	local monitor_cmd

	: >"${MONITOR_LOG}"
	monitor_cmd="$(printf '%q ' cargo run --quiet -p squidc -- target monitor --target "${TARGET_ID}" --port "${PORT}")"
	if command -v script >/dev/null 2>&1; then
		(
			cd "${ROOT}"
			timeout "$((BLE_ADVERTISING_LOG_TIMEOUT_SECONDS + BLE_RESTART_GRACE_SECONDS + BLE_RESCAN_TIMEOUT_SECONDS + 20))s" \
				script -q -e -c "${monitor_cmd}" /dev/null
		) >"${MONITOR_LOG}" 2>&1 &
	else
		(
			cd "${ROOT}"
			timeout "$((BLE_ADVERTISING_LOG_TIMEOUT_SECONDS + BLE_RESTART_GRACE_SECONDS + BLE_RESCAN_TIMEOUT_SECONDS + 20))s" \
				cargo run --quiet -p squidc -- target monitor --target "${TARGET_ID}" --port "${PORT}"
		) >"${MONITOR_LOG}" 2>&1 &
	fi
	MONITOR_PID="$!"
}

wait_for_log_line() {
	local expected="$1"
	local timeout_seconds="$2"
	local deadline=$((SECONDS + timeout_seconds))

	while (( SECONDS < deadline )); do
		if grep -Fq "${expected}" "${MONITOR_LOG}"; then
			return 0
		fi
		sleep 0.5
	done
	printf 'Timed out waiting for serial log line: %s\n' "${expected}" >&2
	printf 'Monitor log: %s\n' "${MONITOR_LOG}" >&2
	exit 1
}

scan_for_device() {
	local scan_file="$1"
	local selector="$2"
	local require_fresh="$3"

	python3 - "${scan_file}" "${selector}" "${require_fresh}" <<'PY'
import re
import sys
path, selector, require_fresh = sys.argv[1:4]
ansi = re.compile(r"\x1b\[[0-9;]*m")
event_prefix = re.compile(r"\[(NEW|CHG|DEL)\]")
device_line = re.compile(r"Device\s+([0-9A-Fa-f:]{17})\s+(.+)$")
name_change = re.compile(r"Device\s+([0-9A-Fa-f:]{17})\s+Name:\s+(.+)$")
truncated = selector.encode("utf-8")[:29].decode("utf-8", "ignore")
with open(path, encoding="utf-8", errors="replace") as handle:
    for raw_line in handle:
        line = ansi.sub("", raw_line)
        event = event_prefix.search(line)
        if event and event.group(1) == "DEL":
            continue
        if require_fresh == "1" and (not event or event.group(1) not in {"NEW", "CHG"}):
            continue
        match = name_change.search(line) or device_line.search(line)
        if not match:
            continue
        address = match.group(1).strip()
        name = match.group(2).strip()
        if selector.lower() == address.lower() or selector in name or truncated in name:
            print(address)
            break
PY
}

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
if [[ -z "${DEVICE_SELECTOR}" ]]; then
	DEVICE_SELECTOR="${DEVICE_NAME}"
fi

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

run_capture launch-fallback-main cargo run --quiet -p squidc -- app launch main >/dev/null
printf '%s\n' 'OK fallback main launched to start BLE profile'
start_serial_monitor

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

BLE_ADDR="$(scan_for_device "${scan_out}" "${DEVICE_SELECTOR}" 0)"
if [[ -z "${BLE_ADDR}" ]]; then
	printf 'Expected host Bluetooth scan to discover %s within %ss\n' \
		"${DEVICE_SELECTOR}" "${BLE_RESCAN_TIMEOUT_SECONDS}" >&2
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
wait_for_log_line "BLE advertising stopped before restart" "${BLE_ADVERTISING_LOG_TIMEOUT_SECONDS}"
wait_for_log_line "BLE advertising restarted after disconnect" "${BLE_ADVERTISING_LOG_TIMEOUT_SECONDS}"
printf '%s\n' 'OK BLE firmware restart logs observed'

bluetoothctl remove "${BLE_ADDR}" >"${WORK_DIR}/ble-remove-before-rescan.out" 2>&1 || true

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

rediscovered_addr="$(scan_for_device "${rescan_out}" "${DEVICE_SELECTOR}" 1)"
if [[ -z "${rediscovered_addr}" ]]; then
	printf 'Expected host Bluetooth to rediscover %s within %ss after disconnect\n' \
		"${DEVICE_SELECTOR}" "${BLE_RESCAN_TIMEOUT_SECONDS}" >&2
	printf 'Rescan log: %s\n' "${rescan_out}" >&2
	exit 1
fi
printf '%s\n' 'OK host rediscovered fresh advertisement after disconnect'

timeout "${BLE_CONNECT_TIMEOUT_SECONDS}s" bluetoothctl disconnect "${BLE_ADDR}" \
	>"${WORK_DIR}/ble-disconnect-final.out" 2>&1 || true
BLE_ADDR=""

printf '%s\n' 'OK Zephyr BLE re-advertising after host disconnect verified'
