#!/usr/bin/env bash
# Push a SQBC file to a SquidScript device's BLE OTS service and verify
# the device installs it. This is the slice 10 hardware test wrapper.
#
# Defaults to the XIAO ESP32-C3 e-paper dev target. The wrapper sources
# scripts/zephyr-env.sh for the Zephyr toolchain, builds and flashes the
# target, compiles the ble-install example, and runs the ots-push driver
# against the device. All skip behavior is encapsulated in ots-push.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/xiao-esp32c3-gdeq0426t82-sd.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/xiao_esp32c3_gdeq0426t82_sd.overlay}"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-object-transfer.sh [-- <extra args>]

  --device <name-or-address>   BLE device name or address (required)
  --port <serial-port>         Serial port for the REPL/CLI (default: auto)
  --source <file.sqbc>         Source SQBC to push (default: ble-install example)
  --skip-flash                 Build only; do not flash the target

Environment:
  SQUID_ZEPHYR_TARGET_JSON      Override the target metadata JSON
  SQUID_ZEPHYR_TARGET_OVERLAY   Override the Zephyr board overlay

The wrapper flashes the XIAO target, compiles examples/ble-install, arms
it via the serial CLI, runs tools/ots-push to push the source SQBC, and
verifies via squidc app list that the new app is registered.
USAGE
}

DEVICE=""
PORT=""
SOURCE=""
SKIP_FLASH=0

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

if [[ -z "$SOURCE" ]]; then
	SOURCE="${ROOT}/examples/ble-install/main.squid"
fi

if [[ -z "$PORT" ]]; then
	PORT="$(ls /dev/ttyACM* /dev/ttyUSB* 2>/dev/null | head -1 || true)"
	if [[ -z "$PORT" ]]; then
		echo "ERROR: no serial port found; pass --port" >&2
		exit 1
	fi
fi

echo ">>> Building XIAO target"
cargo run -p squidc -- target build --target xiao-esp32c3-gdeq0426t82-sd

if [[ "$SKIP_FLASH" -eq 0 ]]; then
	echo ">>> Flashing $PORT"
	squidc device install --port "$PORT"
fi

echo ">>> Compiling ble-install example"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT
EXAMPLE_SQBC="${BUILD_DIR}/ble-install.sqbc"
PAYLOAD_SQBC="${BUILD_DIR}/installed-app.sqbc"
cargo run -p squidc -- app build "${ROOT}/examples/ble-install/main.squid" --out "$EXAMPLE_SQBC"

echo ">>> Installing ble-install example"
squidc app install --port "$PORT" "$EXAMPLE_SQBC" --as ble-install

echo ">>> Arming ble-install"
squidc device repl --port "$PORT" <<< "app.arm ble-install"

echo ">>> Compiling payload (installed-app.sqbc)"
cargo run -p squidc -- app build "${ROOT}/examples/ble-install/main.squid" --out "$PAYLOAD_SQBC"

echo ">>> Pushing payload via BLE OTS to $DEVICE"
python3 -m ots_push push "$DEVICE" ble-install sqbc-install "$PAYLOAD_SQBC"

echo ">>> Verifying installed-app is registered"
squidc app list --port "$PORT" | grep -q "installed-app" || {
	echo "ERROR: installed-app not found in app list" >&2
	exit 1
}

echo ">>> Disarming ble-install"
squidc device repl --port "$PORT" <<< "app.disarm ble-install"

echo "OK ble-object-transfer"
