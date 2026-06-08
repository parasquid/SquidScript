#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
WORK_DIR="${ROOT}/target/hardware-tests/xiao-ram-workloads"
EPAPER_GRAY2_APP_ID="epaper-gray2-smoke"
EPAPER_GRAY2_APP_DIR="${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke"
EPAPER_GRAY2_PACKAGE="${WORK_DIR}/${EPAPER_GRAY2_APP_ID}.squid.zip"
SYSTEM_APP="${ROOT}/tests/hardware/zephyr/system-resources/main.squid"
WIFI_AP_APP="${ROOT}/tests/hardware/zephyr/wifi-ap-summary/main.squid"
REFRESH_DELAY_SECONDS="${SQUID_EPAPER_GRAY2_REFRESH_DELAY_SECONDS:-8}"
SKIP_FLASH="${SKIP_FLASH:-0}"
TARGET_COMMAND_TIMEOUT_SECONDS="${TARGET_COMMAND_TIMEOUT_SECONDS:-180}"

usage() {
  cat <<'USAGE'
Usage: scripts/xiao-esp32c3-measure-ram-workloads.sh [--target <id>] [--skip-flash]

Runs XIAO ESP32-C3 RAM workload attribution and writes summary.tsv under
target/hardware-tests/xiao-ram-workloads.
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

ram_reset_runtime_between_workloads epaper-gray2
ram_reset_heap_max_attribution epaper-gray2
run_capture package-epaper-gray2 \
  cargo run --quiet -p squidc -- app package "${EPAPER_GRAY2_APP_DIR}" \
    --target "${TARGET_ID}" --out "${EPAPER_GRAY2_PACKAGE}" >/dev/null
ram_run_app_capture install-epaper-gray2 install "${EPAPER_GRAY2_PACKAGE}" >/dev/null
ram_run_app_capture launch-epaper-gray2 launch "${EPAPER_GRAY2_APP_ID}" >/dev/null
sleep "${REFRESH_DELAY_SECONDS}"
ram_wait_for_device_contains epaper-gray2-output "gray2 pages 1" \
  "device output" output >/dev/null
ram_wait_for_device_contains epaper-gray2-drawlog "draw=binbook" \
  "device drawlog" drawlog >/dev/null
ram_snapshot_resources epaper-gray2-after-launch

ram_reset_runtime_between_workloads system
ram_reset_heap_max_attribution system
ram_run_app_capture install-system-resources install "${SYSTEM_APP}" >/dev/null
ram_run_app_capture launch-system-resources launch system-resources >/dev/null
ram_wait_for_device_contains system-output "output=system memory RAM" \
  "device output" output >/dev/null
ram_snapshot_resources system-after-launch

ram_reset_runtime_between_workloads wifi-ap
ram_reset_heap_max_attribution wifi-ap-start
ram_run_app_capture install-wifi-ap install "${WIFI_AP_APP}" >/dev/null
ram_run_app_capture launch-wifi-ap launch wifi-ap-summary >/dev/null
ram_wait_for_device_contains wifi-ap-output-start "output=wifi start true null" \
  "device output" output >/dev/null
ram_snapshot_resources wifi-ap-after-start

ram_reset_heap_max_attribution wifi-ap-stop
ram_run_device_capture stop-wifi-ap-key key SELECT >/dev/null
ram_wait_for_device_contains wifi-ap-output-stop "output=wifi stop true null" \
  "device output" output >/dev/null
ram_snapshot_resources wifi-ap-after-stop

printf 'OK XIAO ESP32-C3 RAM workload resources captured: %s\n' "${summary_out}"
