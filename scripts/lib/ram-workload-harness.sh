#!/usr/bin/env bash

source "${ROOT}/scripts/lib/hardware-command.sh"

COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
SYSTEM_HEAP_BYTES="${SYSTEM_HEAP_BYTES:-65536}"
SQ_VM_RUNTIME_COMPLETE_STATUS="${SQ_VM_RUNTIME_COMPLETE_STATUS:-2}"

if ! [[ "${SYSTEM_HEAP_BYTES}" =~ ^[0-9]+$ ]] || (( SYSTEM_HEAP_BYTES < 1 )); then
  printf 'SYSTEM_HEAP_BYTES must be a positive integer\n' >&2
  exit 2
fi

ram_device_command_args() {
  local command=(cargo run --quiet -p squidc -- device "$@")
  if [[ -n "${ESPFLASH_PORT:-}" ]]; then
    command+=(--port "${ESPFLASH_PORT}")
  fi
  printf '%s\0' "${command[@]}"
}

ram_app_command_args() {
  local command=(cargo run --quiet -p squidc -- app "$@")
  if [[ -n "${ESPFLASH_PORT:-}" ]]; then
    command+=(--port "${ESPFLASH_PORT}")
  fi
  printf '%s\0' "${command[@]}"
}

ram_run_device_capture() {
  local label="$1"
  shift
  local command=()
  while IFS= read -r -d '' part; do
    command+=("$part")
  done < <(ram_device_command_args "$@")
  run_capture "$label" "${command[@]}"
}

ram_run_app_capture() {
  local label="$1"
  shift
  local command=()
  while IFS= read -r -d '' part; do
    command+=("$part")
  done < <(ram_app_command_args "$@")
  run_capture "$label" "${command[@]}"
}

ram_wait_for_device_contains() {
  local label="$1"
  local expected="$2"
  local command_name="$3"
  shift 3
  local command=()
  while IFS= read -r -d '' part; do
    command+=("$part")
  done < <(ram_device_command_args "$@")
  ram_wait_for_contains "$label" "$expected" "$command_name" "${command[@]}"
}

ram_wait_for_resource_value() {
  local label="$1"
  local key="$2"
  local expected="$3"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local file=""
  local value=""

  while (( SECONDS < deadline )); do
    if file="$(ram_run_device_capture "$label" resources)" &&
      value="$(ram_resource_value "$file" "$key" 2>/dev/null)" &&
      [[ "$value" == "$expected" ]]; then
      printf '%s\n' "$file"
      return 0
    fi
    sleep 0.2
  done

  printf 'Timed out waiting for resource %s=%s; last value=%s\n' \
    "$key" "$expected" "${value:-<missing>}" >&2
  if [[ -n "$file" && -f "$file" ]]; then
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "$file" >&2
  fi
  exit 1
}

ram_resource_value() {
  local file="$1"
  local key="$2"
  awk -F= -v key="$key" '$1 == key { print $2; found = 1 } END { if (!found) exit 1 }' "$file"
}

ram_assert_stack_accounting() {
  local file="$1"
  local prefix="$2"
  local size unused used
  size="$(ram_resource_value "$file" "${prefix}_stack_size_bytes")"
  unused="$(ram_resource_value "$file" "${prefix}_stack_unused_bytes")"
  used="$(ram_resource_value "$file" "${prefix}_stack_used_bytes")"
  if (( used < 0 || unused < 0 || used + unused != size )); then
    printf 'Invalid %s stack accounting in %s: size=%s used=%s unused=%s\n' \
      "$prefix" "$file" "$size" "$used" "$unused" >&2
    sed -n '1,200p' "$file" >&2
    exit 1
  fi
}

ram_wait_for_contains() {
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

ram_init_summary() {
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
}

ram_snapshot_resources() {
  local label="$1"
  local file
  local heap_alloc heap_max_alloc
  file="$(ram_run_device_capture "resources-${label}" resources)"
  ram_assert_stack_accounting "$file" proto
  ram_assert_stack_accounting "$file" vm
  heap_alloc="$(ram_resource_value "$file" heap_alloc_bytes)"
  heap_max_alloc="$(ram_resource_value "$file" heap_max_alloc_bytes)"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$label" \
    "$(ram_resource_value "$file" proto_stack_pre_used_bytes)" \
    "$(ram_resource_value "$file" proto_stack_used_bytes)" \
    "$(ram_resource_value "$file" proto_stack_unused_bytes)" \
    "$(ram_resource_value "$file" vm_stack_used_bytes)" \
    "$(ram_resource_value "$file" vm_stack_unused_bytes)" \
    "$heap_alloc" \
    "$heap_max_alloc" \
    "$(( SYSTEM_HEAP_BYTES - heap_max_alloc ))" \
    "$(ram_resource_value "$file" heap_largest_free_supported)" \
    "$(ram_resource_value "$file" heap_largest_free_bytes)" \
    "$(ram_resource_value "$file" runtime_static_bytes)" \
    "$(ram_resource_value "$file" last_dispatch_seq)" \
    "$(ram_resource_value "$file" last_dispatch_us)" \
    >>"${summary_out}"
}

ram_reset_heap_max_attribution() {
  local label="$1"
  ram_run_device_capture "heap-reset-${label}" resources --reset-heap-max >/dev/null
  printf 'heap high-water reset before workload: %s\n' "${label}" >&2
}

ram_reset_runtime_between_workloads() {
  local label="$1"
  ram_run_device_capture "reset-${label}" reset >/dev/null
  printf 'runtime reset before independent workload group: %s\n' "${label}" >&2
}
