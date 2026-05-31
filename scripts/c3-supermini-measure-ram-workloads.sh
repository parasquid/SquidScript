#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/ram-workloads"
INPUT_BUTTON_APP="${ROOT}/tests/hardware/c3-supermini/input-button-summary/main.squid"
DISPLAY_APP="${ROOT}/tests/hardware/c3-supermini/display-drawlog/main.squid"
SYSTEM_APP="${ROOT}/tests/hardware/c3-supermini/system-resources/main.squid"
WIFI_AP_APP="${ROOT}/tests/hardware/c3-supermini/wifi-ap-summary/main.squid"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
SYSTEM_HEAP_BYTES="${SYSTEM_HEAP_BYTES:-51200}"

mkdir -p "${WORK_DIR}"

if ! [[ "$SYSTEM_HEAP_BYTES" =~ ^[0-9]+$ ]] || (( SYSTEM_HEAP_BYTES < 1 )); then
  printf 'SYSTEM_HEAP_BYTES must be a positive integer\n' >&2
  exit 2
fi

resource_value() {
  local file="$1"
  local key="$2"
  awk -F= -v key="$key" '$1 == key { print $2; found = 1 } END { if (!found) exit 1 }' "$file"
}

assert_stack_accounting() {
  local file="$1"
  local prefix="$2"
  local size unused used
  size="$(resource_value "$file" "${prefix}_stack_size_bytes")"
  unused="$(resource_value "$file" "${prefix}_stack_unused_bytes")"
  used="$(resource_value "$file" "${prefix}_stack_used_bytes")"
  if (( used < 0 || unused < 0 || used + unused != size )); then
    printf 'Invalid %s stack accounting in %s: size=%s used=%s unused=%s\n' \
      "$prefix" "$file" "$size" "$used" "$unused" >&2
    sed -n '1,200p' "$file" >&2
    exit 1
  fi
}

wait_for_contains() {
  local label="$1"
  local expected="$2"
  local command_name="$3"
  shift 3
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))

  while (( SECONDS < deadline )); do
    if timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@" >"${out}" 2>&1 &&
      grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for %s in %s\n' "${expected}" "${command_name}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

snapshot_resources() {
  local label="$1"
  local file
  local heap_alloc heap_max_alloc
  file="$(run_capture "resources-${label}" cargo run --quiet -p squidc -- device resources)"
  assert_stack_accounting "$file" proto
  assert_stack_accounting "$file" vm
  heap_alloc="$(resource_value "$file" heap_alloc_bytes)"
  heap_max_alloc="$(resource_value "$file" heap_max_alloc_bytes)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$label" \
    "$(resource_value "$file" proto_stack_pre_used_bytes)" \
    "$(resource_value "$file" proto_stack_used_bytes)" \
    "$(resource_value "$file" proto_stack_unused_bytes)" \
    "$(resource_value "$file" vm_stack_used_bytes)" \
    "$(resource_value "$file" vm_stack_unused_bytes)" \
    "$heap_alloc" \
    "$heap_max_alloc" \
    "$(( SYSTEM_HEAP_BYTES - heap_max_alloc ))" \
    "$(resource_value "$file" heap_largest_free_supported)" \
    "$(resource_value "$file" heap_largest_free_bytes)" \
    "$(resource_value "$file" runtime_static_bytes)" \
    "$(resource_value "$file" last_dispatch_seq)" \
    "$(resource_value "$file" last_dispatch_us)" \
    >>"${summary_out}"
}

reset_runtime_between_workloads() {
  local label="$1"
  run_capture "reset-${label}" cargo run --quiet -p squidc -- device reset >/dev/null
  printf 'runtime reset before independent workload group: %s\n' "${label}" >&2
}

summary_out="${WORK_DIR}/summary.tsv"
{
  printf 'workload\tproto_stack_pre_used_bytes\t'
  printf 'proto_stack_used_bytes\t'
  printf 'proto_stack_unused_bytes\t'
  printf 'vm_stack_used_bytes\tvm_stack_unused_bytes\t'
  printf 'heap_alloc_bytes\theap_max_alloc_bytes\t'
  printf 'heap_max_headroom_bytes\t'
  printf 'heap_largest_free_supported\theap_largest_free_bytes\t'
  printf 'runtime_static_bytes\tlast_dispatch_seq\tlast_dispatch_us\n'
} >"${summary_out}"

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
snapshot_resources after-format

run_capture install-input-button cargo run --quiet -p squidc -- app install "${INPUT_BUTTON_APP}" >/dev/null
snapshot_resources input-after-install

run_capture launch-input-button cargo run --quiet -p squidc -- app launch input-button-summary >/dev/null
wait_for_contains input-output-start "output=count 0" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources input-after-launch

run_capture key-select cargo run --quiet -p squidc -- device key SELECT >/dev/null
wait_for_contains input-output-select "output=count 1" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources input-after-select

reset_runtime_between_workloads display
run_capture install-display-drawlog cargo run --quiet -p squidc -- app install "${DISPLAY_APP}" >/dev/null
run_capture launch-display-drawlog cargo run --quiet -p squidc -- app launch display-drawlog >/dev/null
wait_for_contains display-drawlog 'draw=resource drawable="drawable/page" x=0 y=0' \
  "device drawlog" cargo run --quiet -p squidc -- device drawlog >/dev/null
snapshot_resources display-after-launch

reset_runtime_between_workloads system
run_capture install-system-resources cargo run --quiet -p squidc -- app install "${SYSTEM_APP}" >/dev/null
run_capture launch-system-resources cargo run --quiet -p squidc -- app launch system-resources >/dev/null
wait_for_contains system-output "output=system memory RAM" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources system-after-launch

reset_runtime_between_workloads wifi-ap
run_capture install-wifi-ap cargo run --quiet -p squidc -- app install "${WIFI_AP_APP}" >/dev/null
run_capture launch-wifi-ap cargo run --quiet -p squidc -- app launch wifi-ap-summary >/dev/null
wait_for_contains wifi-ap-output-start "output=wifi start true null" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources wifi-ap-after-start

run_capture stop-wifi-ap-key cargo run --quiet -p squidc -- device key SELECT >/dev/null
wait_for_contains wifi-ap-output-stop "output=wifi stop true null" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources wifi-ap-after-stop

printf 'OK ESP32-C3 RAM workload resources captured: %s\n' "${summary_out}"
