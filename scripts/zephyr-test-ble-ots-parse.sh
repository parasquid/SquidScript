#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-ots-parse.sh [-- <extra twister args>]

Runs the BLE OTS Object Name parser ztests through Twister on
native_sim/native/64. The tests validate the app_id/profile_id/.ext
Object Name shape used to route an in-flight transfer to the armed
SquidScript app, with host-side rule coverage (empty segments, unsafe
app_id, missing extension dot, buffer overflow) on the host filesystem
without real Bluetooth.
USAGE
}

EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			usage
			exit 0
			;;
		--)
			shift
			EXTRA_ARGS=("$@")
			break
			;;
		*)
			EXTRA_ARGS+=("$1")
			;;
	esac
	shift
done

west twister \
	-T "${ROOT}/firmware/zephyr/tests/ble-ots-parse" \
	--platform native_sim/native/64 \
	--inline-logs \
	"${EXTRA_ARGS[@]}"
