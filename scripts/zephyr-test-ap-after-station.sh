#!/usr/bin/env bash
# TDD helper: drive the device through AP start, AP stop, station
# connect+disconnect, AP start in a single runtime session and check the
# second AP start does not report "ap ip failed". This reproduces the
# ESP32-C3 Wi-Fi AP-after-station teardown bug, and after the fix is in
# place it confirms a clean restart is possible.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
SKIP_FLASH="${SKIP_FLASH:-0}"
AP_AFTER_STATION_TIMEOUT_SECONDS="${AP_AFTER_STATION_TIMEOUT_SECONDS:-30}"
AP_AFTER_STATION_RESET_ATTEMPTS="${AP_AFTER_STATION_RESET_ATTEMPTS:-3}"
AP_AFTER_STATION_RESET_DELAY_SECONDS="${AP_AFTER_STATION_RESET_DELAY_SECONDS:-2}"
APP_SRC="${ROOT}/tests/hardware/zephyr/ap-after-station/main.squid"
APP_ID="ap-after-station"
HOST_AP_SSID="${HOST_AP_SSID:-SquidApAfterStation}"
HOST_AP_CONN="${HOST_AP_CONN:-squid-ap-after-station-host}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
HOST_WIFI_RECONNECT_CONN=""
HOST_AP_PASS_FILE=""
WORK_DIR="${ROOT}/target/hardware-tests/ap-after-station"

usage() {
	cat <<'EOF'
Usage: scripts/zephyr-test-ap-after-station.sh [--target <id>] [--skip-flash]

Builds or flashes the Zephyr firmware, sets up a temporary host AP, saves
the device wifi profile, runs the ap-after-station app on the target, and
asserts that the second AP start (after station connect+disconnect)
reports ok=true (not "ap ip failed"). Targets a single VM session so the
runtime state from the first AP start and station teardown is still in
place when the second AP start runs.

Requires a host Wi-Fi interface managed by NetworkManager. Do not run in
parallel with any other firmware, monitor, or hardware command against
the same physical target.
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

cleanup() {
	set +e
	if [[ -n "${HOST_AP_PASS_FILE}" ]] && [[ -s "${HOST_AP_PASS_FILE}" ]]; then
		nmcli connection down "${HOST_AP_CONN}" >"${WORK_DIR}/cleanup-host-ap-down.out" 2>&1 || true
		nmcli connection delete "${HOST_AP_CONN}" >"${WORK_DIR}/cleanup-host-ap-delete.out" 2>&1 || true
		if [[ -n "${HOST_WIFI_IFACE}" ]]; then
			if [[ -n "${HOST_WIFI_RECONNECT_CONN}" ]]; then
				nmcli connection up "${HOST_WIFI_RECONNECT_CONN}" \
					>"${WORK_DIR}/cleanup-host-wifi-connect.out" 2>&1 || true
			else
				nmcli device connect "${HOST_WIFI_IFACE}" \
					>"${WORK_DIR}/cleanup-host-wifi-connect.out" 2>&1 || true
			fi
		fi
		rm -f "${HOST_AP_PASS_FILE}"
	fi
}
trap cleanup EXIT

if ! command -v nmcli >/dev/null 2>&1; then
	printf '%s\n' 'nmcli is required for the AP-after-station check' >&2
	exit 1
fi

run_reset_with_recovery() {
	local label="$1"
	local status=0

	timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
		cargo run --quiet -p squidc -- device resources \
		>"${WORK_DIR}/${label}-hello-before.out" 2>&1 || true

	for attempt in $(seq 1 "${AP_AFTER_STATION_RESET_ATTEMPTS}"); do
		set +e
		timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
			cargo run --quiet -p squidc -- device reset \
			>"${WORK_DIR}/${label}.out" 2>&1
		status=$?
		set -e
		if [[ "${status}" == "0" ]]; then
			cat "${WORK_DIR}/${label}.out"
			return 0
		fi
		capture_device_diagnostics "${label}-attempt-${attempt}"
		if (( attempt < AP_AFTER_STATION_RESET_ATTEMPTS )); then
			if [[ "${status}" == "124" ]] ||
				grep -Eq 'busy \(-16\)|firmware did not become ready|BadMagic|TruncatedHeader|LengthMismatch' \
					"${WORK_DIR}/${label}.out"; then
				sleep "${AP_AFTER_STATION_RESET_DELAY_SECONDS}"
				continue
			fi
		fi
		break
	done

	capture_raw_serial_diagnostics "${label}"
	printf 'Command failed or timed out during AP-after-station reset recovery: device reset\n' >&2
	printf '%s\n' "--- ${WORK_DIR}/${label}.out ---" >&2
	sed -n '1,200p' "${WORK_DIR}/${label}.out" >&2
	printf 'failure diagnostics: %s %s %s %s\n' \
		"${WORK_DIR}/${label}-hello-before.out" \
		"${WORK_DIR}/${label}-raw-serial.out" \
		"${WORK_DIR}/${label}-attempt-${attempt}-resources.out" \
		"${WORK_DIR}/${label}-attempt-${attempt}-errors.out" >&2
	return "${status}"
}

PORT="$(resolve_esp_serial_port)"
export ESPFLASH_PORT="${ESPFLASH_PORT:-${PORT}}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
	printf '%s: cargo run --quiet -p squidc -- target flash --target %s\n' \
		"${HARDWARE_COMMAND_LABEL}" "${TARGET_ID}" >&2
	COMMAND_TIMEOUT_SECONDS="${AP_AFTER_STATION_FLASH_TIMEOUT_SECONDS:-300}" \
		cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" \
		>"${WORK_DIR}/target-flash.out" 2>&1
else
	run_capture target-build cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
fi

if [[ -z "${HOST_WIFI_IFACE}" ]]; then
	HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status |
		awk -F: '$2 == "wifi" { print $1; exit }')"
fi
if [[ -z "${HOST_WIFI_IFACE}" ]]; then
	printf '%s\n' 'No host Wi-Fi interface found for AP-after-station test' >&2
	exit 1
fi
HOST_WIFI_RECONNECT_CONN="$(nmcli -t -f NAME,DEVICE connection show --active |
	awk -F: -v iface="${HOST_WIFI_IFACE}" '$2 == iface { print $1; exit }')"
if [[ -z "${HOST_WIFI_RECONNECT_CONN}" ]]; then
	HOST_WIFI_RECONNECT_CONN="$(nmcli -t -f NAME,TYPE connection show |
		awk -F: -v host_conn="${HOST_AP_CONN}" '$2 == "802-11-wireless" && $1 != host_conn { print $1; exit }')"
fi

HOST_AP_PASS_FILE="$(mktemp "${WORK_DIR}/host-ap-pass.XXXXXX")"
local_password="$(python3 - <<'PY'
import secrets
import string
alphabet = string.ascii_letters + string.digits
print("Sq" + "".join(secrets.choice(alphabet) for _ in range(14)) + "9")
PY
)"
printf '%s' "${local_password}" >"${HOST_AP_PASS_FILE}"

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

run_reset_with_recovery reset-before-ap-after-station >/dev/null

export SQUID_WIFI_STATION_SSID="${HOST_AP_SSID}"
export SQUID_WIFI_STATION_PASSWORD="${local_password}"
run_capture wifi-profile cargo run --quiet -p squidc -- device wifi-profile dev \
	--ssid-env SQUID_WIFI_STATION_SSID \
	--password-env SQUID_WIFI_STATION_PASSWORD >/dev/null
unset SQUID_WIFI_STATION_SSID SQUID_WIFI_STATION_PASSWORD

install_app_out="$(run_capture install-app cargo run --quiet -p squidc -- app install "${APP_SRC}")"
if ! grep -Fq "installed app ${APP_ID}" "${install_app_out}"; then
	printf 'Expected app install to report installed app %s; got: %s\n' "${APP_ID}" "${install_app_out}" >&2
	exit 1
fi

run_capture launch-app cargo run --quiet -p squidc -- app launch "${APP_ID}" >/dev/null

wait_for_contains() {
	local label="$1"
	local expected="$2"
	local deadline=$((SECONDS + AP_AFTER_STATION_TIMEOUT_SECONDS))
	local out="${WORK_DIR}/${label}.out"
	while (( SECONDS < deadline )); do
		timeout 5s cargo run --quiet -p squidc -- device output >"${out}" 2>&1
		if grep -Fq "${expected}" "${out}"; then
			printf '%s\n' "${out}"
			return 0
		fi
		sleep 0.5
	done
	printf 'Timed out waiting for %s\n' "${expected}" >&2
	printf 'Last output:\n' >&2
	cat "${out}" >&2
	return 1
}

wait_for_contains ap1-start "ap1 start true" || exit 1
wait_for_contains ap1-stop "ap1 stop true" || exit 1
wait_for_contains connect1 "connect1 true null" || exit 1

output_log="${WORK_DIR}/output-ap-after-station.out"
: >"${output_log}"
cat "${WORK_DIR}/ap1-start.out" >>"${output_log}"
cat "${WORK_DIR}/ap1-stop.out" >>"${output_log}"
cat "${WORK_DIR}/connect1.out" >>"${output_log}"

ap2_out="$(wait_for_contains ap2-start "ap2 start true" || true)"
if [[ -z "${ap2_out}" ]]; then
	printf 'Second AP start did not report ok=true (regression of ap ip failed bug)\n' >&2
	printf '--- full output ---\n' >&2
	cat "${output_log}" >&2
	exit 1
fi
cat "${ap2_out}" >>"${output_log}"

if grep -Fq "ap ip failed" "${output_log}"; then
	printf 'A later AP start reported ap ip failed; log: %s\n' "${output_log}" >&2
	exit 1
fi

cat "${output_log}"
printf '%s\n' 'OK ESP32-C3 AP start after station teardown survives without ap ip failed'
