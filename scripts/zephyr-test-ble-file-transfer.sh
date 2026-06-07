#!/usr/bin/env bash
# Push a SQBC file to a SquidScript device's custom GATT file-transfer
# service and verify the device installs it. Hardware test wrapper.
#
# Builds and optionally flashes the selected Zephyr target with the default
# fallback installer, runs the squidc BLE push driver against the device, and
# verifies the installed payload is registered.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-file-transfer.sh [--target <id>] [-- <extra args>]

  --target <id>                Target metadata id (default: xiao-esp32c3-gdeq0426t82-sd)
  --device <name-or-address>   BLE device name or address (required)
  --port <serial-port>         Serial port (default: auto-detect)
  --source <file.squid>        Source app to compile and push (default: hello example)
  --skip-flash                 Skip the west flash step (assume firmware is already on the device)
  --payload-id <id>            Expected app id from SQBC metadata (default: hello)

Environment:
  SQUID_ZEPHYR_TARGET_JSON      Override the target metadata JSON
  SQUID_ZEPHYR_TARGET_OVERLAY   Override the Zephyr board overlay

The wrapper builds the selected target, flashes it unless --skip-flash is set,
launches the default fallback main app, pushes the compiled SQBC via `squidc
app push` over the custom BLE GATT transfer service, and verifies the installed
payload is registered.
USAGE
}

DEVICE=""
PORT=""
SOURCE=""
SKIP_FLASH=0
PAYLOAD_ID="hello"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

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
		--target)
			TARGET_ID="$2"
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

if [[ -z "$PORT" ]]; then
	PORT="$(resolve_esp_serial_port 2>/dev/null || true)"
	if [[ -z "$PORT" ]]; then
		PORT="$(ls /dev/ttyACM* /dev/ttyUSB* 2>/dev/null | head -1 || true)"
	fi
	if [[ -z "$PORT" ]]; then
		echo "ERROR: no serial port found; pass --port" >&2
		exit 1
	fi
fi

export ESPFLASH_PORT="$PORT"

if [[ -z "$DEVICE" ]]; then
	TARGET_INSPECT_JSON="${WORK_DIR}/target-inspect.json"
	cargo run --quiet -p squidc -- --json target inspect --target "$TARGET_ID" >"$TARGET_INSPECT_JSON"
	DEVICE="$(python3 - "$TARGET_INSPECT_JSON" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    print(json.load(handle)["data"]["name"])
PY
)"
fi

if [[ -z "$SOURCE" ]]; then
	SOURCE="${ROOT}/examples/hello/main.squid"
fi

PAYLOAD_SQBC="${WORK_DIR}/${PAYLOAD_ID}.sqbc"

echo ">>> Building ${TARGET_ID}"
cargo run -p squidc -- target build --target "$TARGET_ID"

if [[ "$SKIP_FLASH" -eq 0 ]]; then
	echo ">>> Flashing ${TARGET_ID} on $PORT"
	cargo run -p squidc -- target flash --target "$TARGET_ID"
fi

echo ">>> Formatting app storage on ${PORT}"
cargo run --quiet -p squidc -- device storage-format --port "$PORT"

echo ">>> Launching fallback main (starts BLE receive on app.start)"
cargo run --quiet -p squidc -- app launch main --port "$PORT"

echo ">>> Compiling payload (${PAYLOAD_ID}) from ${SOURCE}"
cargo run --quiet -p squidc -- app build "${SOURCE}" --out "$PAYLOAD_SQBC"

echo ">>> Pushing payload via BLE to $DEVICE"
cargo run --quiet -p squidc -- app push "$DEVICE" "$PAYLOAD_SQBC"

echo ">>> Verifying ${PAYLOAD_ID} is registered"
cargo run --quiet -p squidc -- app list --port "$PORT" | grep -q "${PAYLOAD_ID}" || {
	echo "ERROR: ${PAYLOAD_ID} not found in app list" >&2
	exit 1
}

echo "OK ble-file-transfer"
