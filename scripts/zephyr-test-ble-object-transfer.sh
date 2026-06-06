#!/usr/bin/env bash
# Push a SQBC file to a SquidScript device's BLE OTS service and verify
# the device installs it. This is the slice 10 hardware test wrapper.
#
# Defaults to the XIAO ESP32-C3 e-paper dev target. The wrapper builds
# and flashes the firmware via west flash, installs the ble-install
# example, launches it (which arms itself on app.start), runs the
# ots-push driver against the device, and verifies the installed
# payload is registered. Skip behavior is fully encapsulated in
# ots-push: a host without a usable BLE adapter exits 0 cleanly.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/xiao-esp32c3-gdeq0426t82-sd.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/xiao_esp32c3_gdeq0426t82_sd.overlay}"
export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/xiao-esp32c3-gdeq0426t82-sd}"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-object-transfer.sh [-- <extra args>]

  --device <name-or-address>   BLE device name or address (required)
  --port <serial-port>         Serial port (default: auto-detect)
  --source <file.squid>        Source SQBC to push (default: ble-install example)
  --skip-flash                 Skip the west flash step (assume firmware is already on the device)
  --app-id <id>                App id for the armed example (default: ble-install)
  --payload-id <id>            App id for the installed payload (default: installed-app)

Environment:
  SQUID_ZEPHYR_TARGET_JSON      Override the target metadata JSON
  SQUID_ZEPHYR_TARGET_OVERLAY   Override the Zephyr board overlay

The wrapper builds the XIAO target, flashes it via west flash, installs
the ble-install example, launches it (which arms itself on app.start),
pushes the source SQBC via tools/ots-push over BLE OTS, and verifies the
installed payload is registered.
USAGE
}

DEVICE=""
PORT=""
SOURCE=""
SKIP_FLASH=0
APP_ID="ble-install"
PAYLOAD_ID="installed-app"

while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			usage
			exit 0
			;;
		--device)
			DEVICE="$2"
			shift
			;;
		--port)
			PORT="$2"
			shift
			;;
		--source)
			SOURCE="$2"
			shift
			;;
		--skip-flash)
			SKIP_FLASH=1
			;;
		--app-id)
			APP_ID="$2"
			shift
			;;
		--payload-id)
			PAYLOAD_ID="$2"
			shift
			;;
		*)
			echo "unknown arg: $1" >&2
			usage
			exit 64
			;;
	esac
	shift
done

if [[ -z "$DEVICE" ]]; then
	echo "ERROR: --device is required (BLE device name or address)" >&2
	usage
	exit 64
fi

if [[ -z "$PORT" ]]; then
	PORT="$(resolve_esp.serial_port 2>/dev/null || true)"
	if [[ -z "$PORT" ]]; then
		PORT="$(ls /dev/ttyACM* /dev/ttyUSB* 2>/dev/null | head -1 || true)"
	fi
	if [[ -z "$PORT" ]]; then
		echo "ERROR: no serial port found; pass --port" >&2
		exit 1
	fi
fi

export ESPFLASH_PORT="$PORT"

if [[ -z "$SOURCE" ]]; then
	SOURCE="${ROOT}/examples/ble-install/main.squid"
fi

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
EXAMPLE_SQBC="${WORK_DIR}/${APP_ID}.sqbc"
PAYLOAD_SQBC="${WORK_DIR}/${PAYLOAD_ID}.sqbc"

echo ">>> Building XIAO target"
cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd

if [[ "$SKIP_FLASH" -eq 0 ]]; then
	echo ">>> Flashing $PORT via west flash"
	west flash -d "${ZEPHYR_BUILD_DIR}"
fi

echo ">>> Compiling armed example (${APP_ID})"
cargo run --quiet -p squidc -- app build "${ROOT}/examples/ble-install/main.squid" --out "$EXAMPLE_SQBC"

echo ">>> Installing armed example"
cargo run --quiet -p squidc -- app install "$EXAMPLE_SQBC"

echo ">>> Launching ${APP_ID} (arms itself on app.start)"
cargo run --quiet -p squidc -- app launch "${APP_ID}"

echo ">>> Compiling payload (${PAYLOAD_ID})"
cargo run --quiet -p squidc -- app build "${ROOT}/examples/ble-install/main.squid" --out "$PAYLOAD_SQBC"

echo ">>> Pushing payload via BLE OTS to $DEVICE"
cd "${ROOT}/tools/ots-push"
python3 -m ots_push push "$DEVICE" "${APP_ID}" sqbc-install "$PAYLOAD_SQBC"
cd "$ROOT"

echo ">>> Verifying ${PAYLOAD_ID} is registered"
cargo run --quiet -p squidc -- app list | grep -q "${PAYLOAD_ID}" || {
	echo "ERROR: ${PAYLOAD_ID} not found in app list" >&2
	exit 1
}

echo "OK ble-object-transfer"
