#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-trigger-table.sh [-- <extra twister args>]

Runs the BLE profile trigger-table ztests through Twister on
native_sim/native/64. The tests verify add/remove/reset/lookup
behavior and the SQ_VM_RUNTIME_BLE_PROFILE_MAX=2 cap on the
table that drives BLE Object Transfer dispatch. No real Bluetooth
or VM is required.
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
	-T "${ROOT}/firmware/zephyr/tests/ble-trigger-table" \
	--platform native_sim/native/64 \
	--inline-logs \
	"${EXTRA_ARGS[@]}"
