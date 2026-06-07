#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
APP_DIR="${ROOT}/tests/hardware/xiao-esp32c3/epaper-hello"
BUILD_DIR="${ROOT}/target/hardware-tests/xiao-epaper-hello/build"
WORK_DIR="${ROOT}/target/hardware-tests/xiao-epaper-hello"
MONITOR_TIMEOUT_SECONDS="${SQUID_EPAPER_HELLO_MONITOR_TIMEOUT_SECONDS:-90}"
SKIP_FLASH=0

usage() {
	cat <<'USAGE'
Usage: scripts/xiao-esp32c3-test-epaper-hello.sh [--skip-flash]

Builds and flashes a diagnostic-only XIAO ESP32-C3 e-paper smoke-test app.
The app bypasses SquidScript and product firmware, drives the SSD1677 panel
directly, and prints EPAPER_HELLO_READY after the refresh command completes.

Passing the serial marker is not full success by itself: visually confirm that
the GDEQ0426T82 panel shows HELLO WORLD with a border and black bars, and that
the text is not mirrored.
USAGE
}

while [[ $# -gt 0 ]]; do
	case "$1" in
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

source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

mkdir -p "$WORK_DIR"

export ZEPHYR_BOARD="${ZEPHYR_BOARD:-xiao_esp32c3}"
export ZEPHYR_PRISTINE="${ZEPHYR_PRISTINE:-always}"

west build \
	-b "${ZEPHYR_BOARD}" \
	-d "${BUILD_DIR}" \
	-p "${ZEPHYR_PRISTINE}" \
	"${APP_DIR}" \
	-- -DDTC_OVERLAY_FILE="${APP_DIR}/boards/xiao_esp32c3.overlay"

if [[ "$SKIP_FLASH" != "1" ]]; then
	export ESPFLASH_PORT="$(resolve_esp_serial_port)"
	west flash -d "$BUILD_DIR"
	sleep 2
fi

PORT="$(resolve_esp_serial_port)"
monitor_log="${WORK_DIR}/monitor.log"
monitor_cmd=(cargo run --quiet -p squidc -- target monitor --target "$TARGET_ID" --port "$PORT")
monitor_shell_command="$(printf '%q ' "${monitor_cmd[@]}")"

set +e
if command -v script >/dev/null 2>&1; then
	(
		cd "$ROOT"
		timeout "${MONITOR_TIMEOUT_SECONDS}s" script -q -e -c "$monitor_shell_command" /dev/null
	) 2>&1 | tee "$monitor_log" | grep -F -m 1 "EPAPER_HELLO_READY" >/dev/null
	pipeline_status=("${PIPESTATUS[@]}")
else
	(
		cd "$ROOT"
		timeout "${MONITOR_TIMEOUT_SECONDS}s" "${monitor_cmd[@]}"
	) 2>&1 | tee "$monitor_log" | grep -F -m 1 "EPAPER_HELLO_READY" >/dev/null
	pipeline_status=("${PIPESTATUS[@]}")
fi
monitor_status="${pipeline_status[0]}"
grep_status="${pipeline_status[2]}"
set -e

if [[ "$grep_status" != "0" ]]; then
	printf 'Expected EPAPER_HELLO_READY not found within %ss on %s.\n' \
		"$MONITOR_TIMEOUT_SECONDS" "$PORT" >&2
	printf 'Monitor log: %s\n' "$monitor_log" >&2
	exit 1
fi

if [[ "$monitor_status" != "0" && "$monitor_status" != "124" && "$grep_status" != "0" ]]; then
	printf 'E-paper hello monitor exited with status %s after log capture.\n' \
		"$monitor_status" >&2
	printf 'Monitor log: %s\n' "$monitor_log" >&2
	exit "$monitor_status"
fi

printf '%s\n' 'OK XIAO e-paper hello serial marker reached; visual confirmation required: HELLO WORLD with border and black bars, not mirrored'
