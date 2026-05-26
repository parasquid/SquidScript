#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/ram-workloads"
INPUT_BUTTON_APP="${ROOT}/tests/hardware/c3-supermini/input-button-summary/main.squid"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware RAM workload: %s\n' "$*" >&2
  timeout "${COMMAND_TIMEOUT_SECONDS}s" "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

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

snapshot_resources() {
  local label="$1"
  local file
  file="$(run_capture "resources-${label}" cargo run --quiet -p squidc -- device resources)"
  assert_stack_accounting "$file" protocol_thread
  assert_stack_accounting "$file" vm_worker
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$label" \
    "$(resource_value "$file" protocol_thread_stack_used_bytes)" \
    "$(resource_value "$file" protocol_thread_stack_unused_bytes)" \
    "$(resource_value "$file" vm_worker_stack_used_bytes)" \
    "$(resource_value "$file" vm_worker_stack_unused_bytes)" \
    "$(resource_value "$file" ram_heap_allocated_bytes)" \
    "$(resource_value "$file" ram_heap_max_allocated_bytes)" \
    "$(resource_value "$file" runtime_static_bytes)" \
    "$(resource_value "$file" last_dispatch_sequence)" \
    "$(resource_value "$file" last_dispatch_elapsed_us)" \
    >>"${summary_out}"
}

summary_out="${WORK_DIR}/summary.tsv"
{
  printf 'workload\tprotocol_thread_stack_used_bytes\t'
  printf 'protocol_thread_stack_unused_bytes\t'
  printf 'vm_worker_stack_used_bytes\tvm_worker_stack_unused_bytes\t'
  printf 'ram_heap_allocated_bytes\tram_heap_max_allocated_bytes\t'
  printf 'runtime_static_bytes\tlast_dispatch_sequence\tlast_dispatch_elapsed_us\n'
} >"${summary_out}"

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
snapshot_resources after-format

run_capture install-input-button cargo run --quiet -p squidc -- app install "${INPUT_BUTTON_APP}" >/dev/null
snapshot_resources after-install

run_capture launch-input-button cargo run --quiet -p squidc -- app launch input-button-summary >/dev/null
snapshot_resources after-launch

run_capture key-select cargo run --quiet -p squidc -- device key SELECT >/dev/null
snapshot_resources after-select

printf 'OK ESP32-C3 RAM workload resources captured: %s\n' "${summary_out}"
