#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
WORK_DIR="${ROOT}/target/hardware-tests/stack-usage"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-20}"
PROTOCOL_STACK_MIN_UNUSED_BYTES="${PROTOCOL_STACK_MIN_UNUSED_BYTES:-768}"
WORKER_STACK_MIN_UNUSED_BYTES="${WORKER_STACK_MIN_UNUSED_BYTES:-384}"

mkdir -p "${WORK_DIR}"

resources_out="${WORK_DIR}/resources-after-workloads.out"
summary_out="${WORK_DIR}/summary.out"

printf 'hardware stack usage: cargo run --quiet -p squidc -- device resources\n' >&2
timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
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
protocol_stack_size="$(resource_value protocol_thread_stack_size_bytes)"
protocol_stack_unused="$(resource_value protocol_thread_stack_unused_bytes)"
protocol_stack_used="$(resource_value protocol_thread_stack_used_bytes)"
protocol_stack_pre_resources_unused="$(
  resource_value protocol_thread_stack_pre_resources_unused_bytes
)"
protocol_stack_pre_resources_used="$(
  resource_value protocol_thread_stack_pre_resources_used_bytes
)"

if [[ "$protocol_stack_size" != "3264" ]]; then
  printf 'Expected protocol_thread_stack_size_bytes=3264, got %s\n' \
    "$protocol_stack_size" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

if (( protocol_stack_used < 0 ||
      protocol_stack_unused < 0 ||
      protocol_stack_used + protocol_stack_unused != protocol_stack_size )); then
  printf 'Invalid protocol stack accounting: size=%s used=%s unused=%s\n' \
    "$protocol_stack_size" "$protocol_stack_used" "$protocol_stack_unused" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

if (( protocol_stack_pre_resources_used < 0 ||
      protocol_stack_pre_resources_unused < 0 ||
      protocol_stack_pre_resources_used + protocol_stack_pre_resources_unused !=
        protocol_stack_size )); then
  printf 'Invalid pre-resources protocol stack accounting: size=%s used=%s unused=%s\n' \
    "$protocol_stack_size" "$protocol_stack_pre_resources_used" \
    "$protocol_stack_pre_resources_unused" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

if (( protocol_stack_unused < PROTOCOL_STACK_MIN_UNUSED_BYTES ||
      protocol_stack_pre_resources_unused < PROTOCOL_STACK_MIN_UNUSED_BYTES )); then
  printf 'Protocol stack headroom below %s bytes: unused=%s pre_resources_unused=%s\n' \
    "$PROTOCOL_STACK_MIN_UNUSED_BYTES" "$protocol_stack_unused" \
    "$protocol_stack_pre_resources_unused" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

if [[ "$stack_size" != "18016" ]]; then
  printf 'Expected vm_worker_stack_size_bytes=18016, got %s\n' "$stack_size" >&2
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

if (( stack_unused < WORKER_STACK_MIN_UNUSED_BYTES )); then
  printf 'VM worker stack headroom below %s bytes: unused=%s\n' \
    "$WORKER_STACK_MIN_UNUSED_BYTES" "$stack_unused" >&2
  printf '%s\n' "--- ${resources_out} ---" >&2
  sed -n '1,200p' "${resources_out}" >&2
  exit 1
fi

{
  printf 'protocol_thread_stack_size_bytes=%s\n' "$protocol_stack_size"
  printf 'protocol_thread_stack_used_bytes=%s\n' "$protocol_stack_used"
  printf 'protocol_thread_stack_unused_bytes=%s\n' "$protocol_stack_unused"
  printf 'protocol_thread_stack_pre_resources_used_bytes=%s\n' \
    "$protocol_stack_pre_resources_used"
  printf 'protocol_thread_stack_pre_resources_unused_bytes=%s\n' \
    "$protocol_stack_pre_resources_unused"
  printf 'vm_worker_stack_size_bytes=%s\n' "$stack_size"
  printf 'vm_worker_stack_used_bytes=%s\n' "$stack_used"
  printf 'vm_worker_stack_unused_bytes=%s\n' "$stack_unused"
} >"${summary_out}"

printf 'OK Zephyr stack usage measured: protocol size=%s used=%s unused=%s; worker size=%s used=%s unused=%s\n' \
  "$protocol_stack_size" "$protocol_stack_used" "$protocol_stack_unused" \
  "$stack_size" "$stack_used" "$stack_unused"
