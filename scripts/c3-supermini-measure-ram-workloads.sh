#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/ram-workloads"
INPUT_BUTTON_APP="${ROOT}/tests/hardware/c3-supermini/input-button-summary/main.squid"
DISPLAY_APP="${ROOT}/tests/hardware/c3-supermini/display-drawlog/main.squid"
SYSTEM_APP="${ROOT}/tests/hardware/c3-supermini/system-resources/main.squid"
WIFI_AP_APP="${ROOT}/tests/hardware/c3-supermini/wifi-ap-summary/main.squid"

source "${ROOT}/scripts/lib/ram-workload-harness.sh"

mkdir -p "${WORK_DIR}"
ram_init_summary

ram_reset_heap_max_attribution storage-format
ram_run_device_capture storage-format storage-format >/dev/null
ram_snapshot_resources after-format

ram_reset_heap_max_attribution input-install
ram_run_app_capture install-input-button install "${INPUT_BUTTON_APP}" >/dev/null
ram_snapshot_resources input-after-install

ram_reset_heap_max_attribution input-launch
ram_run_app_capture launch-input-button launch input-button-summary >/dev/null
ram_snapshot_resources input-after-launch

ram_reset_heap_max_attribution input-select
ram_run_device_capture key-select key SELECT >/dev/null
ram_wait_for_device_contains input-output-select "output=count 1" \
  "device output" output >/dev/null
ram_snapshot_resources input-after-select

ram_reset_runtime_between_workloads display
ram_reset_heap_max_attribution display
ram_run_app_capture install-display-drawlog install "${DISPLAY_APP}" >/dev/null
ram_run_app_capture launch-display-drawlog launch display-drawlog >/dev/null
ram_wait_for_device_contains display-drawlog 'draw=resource drawable="drawable/page" x=0 y=0' \
  "device drawlog" drawlog >/dev/null
ram_snapshot_resources display-after-launch

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

printf 'OK ESP32-C3 RAM workload resources captured: %s\n' "${summary_out}"
