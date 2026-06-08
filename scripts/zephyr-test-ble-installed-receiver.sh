#!/usr/bin/env bash
# Verify BLE file transfer routing through an installed foreground receiver.
#
# This complements the fallback installer hardware test: it installs and
# launches a minimal BLE receiver as a normal app registry entry, pushes an
# exiting app over BLE, and verifies the receiver is reactivated after the
# launched app exits.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"

usage() {
	cat <<'USAGE'
Usage: scripts/zephyr-test-ble-installed-receiver.sh [--target <id>] [--device <name-or-address>] [--port <serial-port>] [--skip-flash]

  --target <id>                Target metadata id (default: xiao-esp32c3-gdeq0426t82-sd)
  --device <name-or-address>   BLE device name or address (default: target display name)
  --port <serial-port>         Serial port (default: auto-detect)
  --skip-flash                 Skip the flash step (assume firmware is already on the device)

The wrapper installs tests/hardware/zephyr/ble-installed-receiver as a normal
app, launches it to start BLE receive from an installed-app registry slot,
pushes an exiting app over BLE, verifies the installed receiver handles the
complete event, waits for foreground return, then pushes the exiting app again
to prove BLE receive is active after return.
USAGE
}

DEVICE=""
PORT=""
SKIP_FLASH=0
WORK_DIR="$(mktemp -d)"
BLE_SERIAL_SETUP_ATTEMPTS="${BLE_SERIAL_SETUP_ATTEMPTS:-6}"
BLE_SERIAL_SETUP_DELAY_SECONDS="${BLE_SERIAL_SETUP_DELAY_SECONDS:-2}"
BLE_WAIT_ATTEMPTS="${BLE_WAIT_ATTEMPTS:-30}"
BLE_WAIT_DELAY_SECONDS="${BLE_WAIT_DELAY_SECONDS:-1}"
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

run_serial_setup() {
	local label="$1"
	shift
	local out="${WORK_DIR}/${label}.out"
	local status=0

	for attempt in $(seq 1 "${BLE_SERIAL_SETUP_ATTEMPTS}"); do
		set +e
		"$@" >"${out}" 2>&1
		status=$?
		set -e
		if [[ "${status}" == "0" ]]; then
			cat "${out}"
			return 0
		fi
		if (( attempt < BLE_SERIAL_SETUP_ATTEMPTS )); then
			if grep -Eq 'busy \(-16\)|firmware did not become ready|BadMagic' "${out}"; then
				sleep "${BLE_SERIAL_SETUP_DELAY_SECONDS}"
				continue
			fi
		fi
		break
	done

	cat "${out}" >&2
	return "${status}"
}

wait_for_serial_output() {
	local label="$1"
	local expected="$2"
	local out="${WORK_DIR}/${label}.out"

	for attempt in $(seq 1 "${BLE_WAIT_ATTEMPTS}"); do
		cargo run --quiet -p squidc -- device output --port "$PORT" >"${out}" 2>&1 || true
		if grep -q "$expected" "${out}"; then
			cat "${out}"
			return 0
		fi
		sleep "${BLE_WAIT_DELAY_SECONDS}"
	done

	printf 'Expected device output to contain %s\n' "$expected" >&2
	printf 'Output log: %s\n' "$out" >&2
	cat "${out}" >&2
	return 1
}

wait_for_serial_output_count() {
	local label="$1"
	local expected="$2"
	local min_count="$3"
	local out="${WORK_DIR}/${label}.out"
	local count=0

	for attempt in $(seq 1 "${BLE_WAIT_ATTEMPTS}"); do
		cargo run --quiet -p squidc -- device output --port "$PORT" >"${out}" 2>&1 || true
		count="$(grep -c "$expected" "${out}" || true)"
		if (( count >= min_count )); then
			cat "${out}"
			return 0
		fi
		sleep "${BLE_WAIT_DELAY_SECONDS}"
	done

	printf 'Expected device output to contain %s at least %s times; saw %s\n' \
		"$expected" "$min_count" "$count" >&2
	printf 'Output log: %s\n' "$out" >&2
	cat "${out}" >&2
	return 1
}

wait_for_app_list() {
	local label="$1"
	local expected="$2"
	local out="${WORK_DIR}/${label}.out"

	for attempt in $(seq 1 "${BLE_WAIT_ATTEMPTS}"); do
		cargo run --quiet -p squidc -- app list --port "$PORT" >"${out}" 2>&1 || true
		if grep -q "$expected" "${out}"; then
			cat "${out}"
			return 0
		fi
		sleep "${BLE_WAIT_DELAY_SECONDS}"
	done

	printf 'Expected app list to contain %s\n' "$expected" >&2
	printf 'App list log: %s\n' "$out" >&2
	cat "${out}" >&2
	return 1
}

echo ">>> Building ${TARGET_ID}"
cargo run -p squidc -- target build --target "$TARGET_ID"

if [[ "$SKIP_FLASH" -eq 0 ]]; then
	echo ">>> Flashing ${TARGET_ID} on $PORT"
	cargo run -p squidc -- target flash --target "$TARGET_ID"
fi

RETURN_SQBC="${WORK_DIR}/ble-route-return.sqbc"

echo ">>> Formatting app storage on ${PORT}"
run_serial_setup storage-format cargo run --quiet -p squidc -- device storage-format --port "$PORT"

echo ">>> Installing BLE receiver app"
run_serial_setup install-ble-receiver cargo run --quiet -p squidc -- app install "${ROOT}/tests/hardware/zephyr/ble-installed-receiver/main.squid" --port "$PORT"

echo ">>> Launching installed BLE receiver"
run_serial_setup launch-ble-receiver cargo run --quiet -p squidc -- app launch ble-installed-receiver --port "$PORT"
wait_for_serial_output receiver-ready "output=ble-installed-receiver ready"

echo ">>> Compiling return payload"
cargo run --quiet -p squidc -- app build "${ROOT}/tests/hardware/zephyr/ble-route-return/main.squid" --out "$RETURN_SQBC"

echo ">>> Pushing return payload via BLE to installed receiver"
cargo run --quiet -p squidc -- app push "$DEVICE" "$RETURN_SQBC"
wait_for_serial_output receiver-complete "output=ble-installed-receiver complete sqbc-install"
wait_for_app_list route-return-installed "app=ret "
wait_for_serial_output route-return-active "output=ble route return active"
wait_for_serial_output_count receiver-ready-returned "output=ble-installed-receiver ready" 2

echo ">>> Pushing return payload again after foreground return"
cargo run --quiet -p squidc -- app push "$DEVICE" "$RETURN_SQBC"
wait_for_serial_output_count receiver-complete-again "output=ble-installed-receiver complete sqbc-install" 2
wait_for_serial_output_count route-return-active-again "output=ble route return active" 2

echo "OK ble-installed-receiver"
