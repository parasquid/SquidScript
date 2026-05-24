#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/stack-usage"

mkdir -p "${WORK_DIR}"

resources_out="${WORK_DIR}/resources-after-workloads.out"
summary_out="${WORK_DIR}/summary.out"

printf 'hardware stack usage: cargo run --quiet -p squidc -- device resources\n' >&2
cargo run --quiet -p squidc -- device resources >"${resources_out}" 2>&1

resource_value() {
  local key="$1"
  local value
  value="$(awk -F= -v key="$key" '$1 == key { print $2; found = 1 } END { if (!found) exit 1 }' "${resources_out}")"
  printf '%s\n' "$value"
}

stack_size="$(resource_value vm_worker_stack_size_bytes)"
stack_unused="$(resource_value vm_worker_stack_unused_bytes)"
stack_used="$(resource_value vm_worker_stack_used_bytes)"

if [[ "$stack_size" != "24576" ]]; then
  printf 'Expected vm_worker_stack_size_bytes=24576, got %s\n' "$stack_size" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

if (( stack_used < 0 || stack_unused < 0 || stack_used + stack_unused != stack_size )); then
  printf 'Invalid worker stack accounting: size=%s used=%s unused=%s\n' \
    "$stack_size" "$stack_used" "$stack_unused" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

{
  printf 'vm_worker_stack_size_bytes=%s\n' "$stack_size"
  printf 'vm_worker_stack_used_bytes=%s\n' "$stack_used"
  printf 'vm_worker_stack_unused_bytes=%s\n' "$stack_unused"
} >"${summary_out}"

printf 'OK Zephyr VM worker stack usage measured: size=%s used=%s unused=%s\n' \
  "$stack_size" "$stack_used" "$stack_unused"
