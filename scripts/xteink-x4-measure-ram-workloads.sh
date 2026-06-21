#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ID="${TARGET_ID:-xteink-x4}"
WORK_DIR="${ROOT}/target/hardware-tests/x4-ram-workloads"
SYSTEM_HEAP_BYTES="${SYSTEM_HEAP_BYTES:-65536}"

GRID_CURSOR_APP_ID="grid-cursor"
GRID_CURSOR_APP_DIR="${ROOT}/examples/grid-cursor"
GRID_CURSOR_PACKAGE="${WORK_DIR}/${GRID_CURSOR_APP_ID}.squid.zip"

BINBOOK_READER_APP_ID="binbook-reader"
BINBOOK_READER_APP_DIR="${ROOT}/examples/binbook-reader"
BINBOOK_READER_PACKAGE="${WORK_DIR}/${BINBOOK_READER_APP_ID}.squid.zip"
BINBOOK_FIXTURE="${WORK_DIR}/reader-one.generated.binbook"
BINBOOK_NAME="${BINBOOK_NAME:-reader-one.binbook}"

SYSTEM_APP_DIR="${ROOT}/tests/hardware/zephyr/system-resources"
SYSTEM_PACKAGE="${WORK_DIR}/system-resources.squid.zip"

WIFI_AP_APP_DIR="${ROOT}/tests/hardware/zephyr/wifi-ap-summary"
WIFI_AP_APP_ID="wifi-ap-summary"
WIFI_AP_PACKAGE="${WORK_DIR}/${WIFI_AP_APP_ID}.squid.zip"

SKIP_FLASH="${SKIP_FLASH:-0}"
TARGET_COMMAND_TIMEOUT_SECONDS="${TARGET_COMMAND_TIMEOUT_SECONDS:-180}"
REFRESH_DELAY_SECONDS="${SQUID_X4_REFRESH_DELAY_SECONDS:-6}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-measure-ram-workloads.sh [--target <id>] [--skip-flash]

Runs XTEINK X4 RAM workload attribution and writes summary.tsv under
target/hardware-tests/x4-ram-workloads. Workloads run sequentially against a
single USB serial device: storage-format baseline, grid-cursor, binbook-reader,
system-resources, and Wi-Fi AP start/stop. BLE and HTTP transfer RAM attribution
are covered by scripts/xteink-x4-test-ble-transfer.sh and
scripts/xteink-x4-test-http-transfer.sh when radio-stack heap high-water is
needed beyond the Wi-Fi AP workload.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET_ID="${2:-}"
      if [[ -z "${TARGET_ID}" ]]; then
        usage >&2
        exit 2
      fi
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

source "${ROOT}/scripts/lib/serial-port.sh"
export ESPFLASH_PORT="${ESPFLASH_PORT:-$(resolve_esp_serial_port)}"
source "${ROOT}/scripts/lib/ram-workload-harness.sh"

mkdir -p "${WORK_DIR}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  COMMAND_TIMEOUT_SECONDS="${TARGET_COMMAND_TIMEOUT_SECONDS}" \
    run_capture target-flash cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
  ram_wait_for_device_contains post-flash-output "output=ble installer ready" \
    "device output" output >/dev/null
  ram_wait_for_resource_value post-flash-complete runtime_status "${SQ_VM_RUNTIME_COMPLETE_STATUS}" >/dev/null
fi

ram_init_summary

ram_reset_runtime_between_workloads storage-format
ram_reset_heap_max_attribution storage-format
ram_run_device_capture storage-format storage-format >/dev/null
ram_snapshot_resources after-format

ram_reset_runtime_between_workloads grid-cursor
ram_reset_heap_max_attribution grid-cursor
run_capture package-grid-cursor \
  cargo run --quiet -p squidc -- app package "${GRID_CURSOR_APP_DIR}" \
    --target "${TARGET_ID}" --out "${GRID_CURSOR_PACKAGE}" >/dev/null
ram_run_app_capture install-grid-cursor install "${GRID_CURSOR_PACKAGE}" >/dev/null
ram_run_app_capture launch-grid-cursor launch "${GRID_CURSOR_APP_ID}" >/dev/null
ram_wait_for_device_contains grid-cursor-output "cursor" \
  "device output" output >/dev/null
ram_run_device_capture grid-cursor-key-down key DOWN >/dev/null
ram_wait_for_device_contains grid-cursor-down "cursor 1" \
  "device output" output >/dev/null
ram_snapshot_resources grid-cursor-after-launch

ram_reset_runtime_between_workloads binbook-reader
ram_reset_heap_max_attribution binbook-reader
python3 "${ROOT}/scripts/generate-test-binbook.py" "${BINBOOK_FIXTURE}"
if [[ ! -s "${BINBOOK_FIXTURE}" ]]; then
  printf 'BinBook fixture generation failed: %s\n' "${BINBOOK_FIXTURE}" >&2
  exit 2
fi
run_capture content-put-binbook \
  cargo run --quiet -p squidc -- device content-put "${BINBOOK_FIXTURE}" \
    --name "${BINBOOK_NAME}" --port "${ESPFLASH_PORT}" >/dev/null
run_capture package-binbook-reader \
  cargo run --quiet -p squidc -- app package "${BINBOOK_READER_APP_DIR}" \
    --target "${TARGET_ID}" --out "${BINBOOK_READER_PACKAGE}" >/dev/null
ram_run_app_capture install-binbook-reader install "${BINBOOK_READER_PACKAGE}" >/dev/null
ram_run_app_capture launch-binbook-reader launch "${BINBOOK_READER_APP_ID}" >/dev/null
ram_wait_for_device_contains binbook-library "library" \
  "device output" output >/dev/null
sleep "${REFRESH_DELAY_SECONDS}"
for _ in $(seq 1 20); do
  for _retry in 1 2 3 4 5; do
    if timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- \
      device key DOWN --port "${ESPFLASH_PORT}" >/dev/null 2>&1; then
      break
    fi
    sleep 2
  done
  sleep 0.5
  if timeout "${COMMAND_TIMEOUT_SECONDS}s" cargo run --quiet -p squidc -- \
    device output --port "${ESPFLASH_PORT}" 2>&1 | grep -Fq "${BINBOOK_NAME}"; then
    break
  fi
done
ram_run_device_capture binbook-open-select key SELECT >/dev/null
ram_wait_for_device_contains binbook-page "reader" \
  "device output" output >/dev/null
ram_wait_for_device_contains binbook-drawlog "draw=binbook" \
  "device drawlog" drawlog >/dev/null
sleep "${REFRESH_DELAY_SECONDS}"
ram_snapshot_resources binbook-reader-after-render

ram_reset_runtime_between_workloads system
ram_reset_heap_max_attribution system
run_capture package-system \
  cargo run --quiet -p squidc -- app package "${SYSTEM_APP_DIR}" \
    --target "${TARGET_ID}" --out "${SYSTEM_PACKAGE}" >/dev/null
ram_run_app_capture install-system install "${SYSTEM_PACKAGE}" >/dev/null
ram_run_app_capture launch-system launch system-resources >/dev/null
ram_wait_for_device_contains system-output "output=system memory RAM" \
  "device output" output >/dev/null
ram_snapshot_resources system-after-launch

ram_reset_runtime_between_workloads wifi-ap
ram_reset_heap_max_attribution wifi-ap-start
run_capture package-wifi-ap \
  cargo run --quiet -p squidc -- app package "${WIFI_AP_APP_DIR}" \
    --target "${TARGET_ID}" --out "${WIFI_AP_PACKAGE}" >/dev/null
ram_run_app_capture install-wifi-ap install "${WIFI_AP_PACKAGE}" >/dev/null
ram_run_app_capture launch-wifi-ap launch "${WIFI_AP_APP_ID}" >/dev/null
ram_wait_for_device_contains wifi-ap-output-start "output=wifi start true null" \
  "device output" output >/dev/null
ram_snapshot_resources wifi-ap-after-start

ram_reset_heap_max_attribution wifi-ap-stop
ram_run_device_capture stop-wifi-ap-key key SELECT >/dev/null
ram_wait_for_device_contains wifi-ap-output-stop "output=wifi stop true null" \
  "device output" output >/dev/null
ram_snapshot_resources wifi-ap-after-stop

printf 'OK XTEINK X4 RAM workload resources captured: %s\n' "${summary_out}"
