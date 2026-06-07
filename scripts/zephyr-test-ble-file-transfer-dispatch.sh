#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-file-transfer-dispatch.sh [-- <extra twister args>]

Runs the BLE file-transfer dispatch-handoff ztests through Twister on
native_sim/native/64. The tests verify the producer (final content write
populates the pending slot) and consumer (drain returns app_id +
event and fs_unlinks the staging file) sides of the BLE Object
Transfer dispatch handoff. The tests mount a real host LittleFS
partition under /sqtest and exercise the lifecycle end to end.
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
	-T "${ROOT}/firmware/zephyr/tests/ble-file-transfer-dispatch" \
	--platform native_sim/native/64 \
	--inline-logs \
	"${EXTRA_ARGS[@]}"
