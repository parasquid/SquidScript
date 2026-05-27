#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

WORK_DIR="${ROOT}/target/hardware-tests/input-stack-isolation"
INPUT_BUTTON_APP="${INPUT_BUTTON_APP:-${ROOT}/tests/hardware/c3-supermini/input-button-summary/main.squid}"
INPUT_BUTTON_APP_ID="${INPUT_BUTTON_APP_ID:-input-button-summary}"
INPUT_BUTTON_LABEL="${INPUT_BUTTON_LABEL:-ESP32-C3 Super Mini BOOT/GPIO9 button}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
SKIP_FLASH="${SKIP_FLASH:-0}"

mkdir -p "${WORK_DIR}"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-measure-input-stack-isolation.sh [--skip-flash]

Measures ESP32-C3 stack and RAM high-water data for an input-button app path.
By default the script builds/flashes and verifies a fresh diagnostic
boot before the first resources snapshot so stack high-water rows are not
inherited from earlier workloads in the same firmware boot.

This script requires holding the configured physical input until the script
observes the pressed state, then releasing it when prompted for the final
after-press snapshot. For the default ESP32-C3 Super Mini GPIO9 active-low
path, short GPIO9 to GND if the tiny BOOT button cannot be held reliably.
Override INPUT_BUTTON_APP, INPUT_BUTTON_APP_ID, and INPUT_BUTTON_LABEL to probe
a candidate input binding such as
tests/hardware/c3-supermini/input-button-gpio5-summary/main.squid.

Use --skip-flash only when an already-running firmware image is acceptable.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

export ESPFLASH_PORT="${ESPFLASH_PORT:-$(resolve_esp_serial_port)}"

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

wait_for_contains_or_timeout() {
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
  return 1
}

input_pressed_count() {
  local file="$1"
  local state
  state="$(resource_value "$file" input_button_state)"
  printf '%s\n' $(((state >> 8) & 255))
}

wait_for_input_released() {
  local out="${WORK_DIR}/resources-release-poll.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))

  while (( SECONDS < deadline )); do
    printf 'c3-supermini-measure-input-stack-isolation.sh: %s\n' \
      'cargo run --quiet -p squidc -- device resources' >&2
    if timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
      cargo run --quiet -p squidc -- device resources >"${out}" 2>&1; then
      if (( "$(input_pressed_count "${out}")" == 0 )); then
        return 0
      fi
    fi
    sleep 0.2
  done

  printf 'Timed out waiting for %s release before press check\n' "${INPUT_BUTTON_LABEL}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  return 1
}

wait_for_input_pressed() {
  local out="${WORK_DIR}/resources-press-poll.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))

  while (( SECONDS < deadline )); do
    printf 'c3-supermini-measure-input-stack-isolation.sh: %s\n' \
      'cargo run --quiet -p squidc -- device resources' >&2
    if timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" \
      cargo run --quiet -p squidc -- device resources >"${out}" 2>&1; then
      if (( "$(input_pressed_count "${out}")" > 0 )); then
        return 0
      fi
    fi
    sleep 0.2
  done

  printf 'Timed out waiting for %s pressed state\n' "${INPUT_BUTTON_LABEL}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  return 1
}

snapshot_resources() {
  local label="$1"
  local file
  file="$(run_capture "resources-${label}" cargo run --quiet -p squidc -- device resources)"
  assert_stack_accounting "$file" proto
  assert_stack_accounting "$file" vm
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$label" \
    "$(resource_value "$file" proto_stack_pre_res_used_bytes)" \
    "$(resource_value "$file" proto_stack_used_bytes)" \
    "$(resource_value "$file" proto_stack_unused_bytes)" \
    "$(resource_value "$file" vm_stack_used_bytes)" \
    "$(resource_value "$file" vm_stack_unused_bytes)" \
    "$(resource_value "$file" heap_alloc_bytes)" \
    "$(resource_value "$file" heap_max_alloc_bytes)" \
    "$(resource_value "$file" runtime_static_bytes)" \
    "$(resource_value "$file" last_dispatch_seq)" \
    "$(resource_value "$file" last_dispatch_us)" \
    "$(resource_value "$file" input_button_state)" \
    >>"${summary_out}"
}

summary_out="${WORK_DIR}/summary.tsv"
{
  printf 'workload\tproto_stack_pre_res_used_bytes\t'
  printf 'proto_stack_used_bytes\t'
  printf 'proto_stack_unused_bytes\t'
  printf 'vm_stack_used_bytes\tvm_stack_unused_bytes\t'
  printf 'heap_alloc_bytes\theap_max_alloc_bytes\t'
  printf 'runtime_static_bytes\tlast_dispatch_seq\tlast_dispatch_us\t'
  printf 'input_button_state\n'
} >"${summary_out}"

if [[ "$SKIP_FLASH" != "1" ]]; then
  SQUID_ZEPHYR_DIAGNOSTIC_LOG_DIR="${WORK_DIR}/diagnostic" \
    "${ROOT}/scripts/c3-supermini-zephyr-test-diagnostic.sh"
else
  printf '%s\n' 'Skipping flash; stack high-water may include earlier firmware work.' >&2
fi

snapshot_resources after-boot

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
snapshot_resources after-format

run_capture install-input-button cargo run --quiet -p squidc -- app install "${INPUT_BUTTON_APP}" >/dev/null
snapshot_resources after-install

run_capture launch-input-button cargo run --quiet -p squidc -- app launch "${INPUT_BUTTON_APP_ID}" >/dev/null
wait_for_contains input-output-start "output=count 0" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null
snapshot_resources after-launch

printf 'Release %s now.\n' "${INPUT_BUTTON_LABEL}" >&2
if ! wait_for_input_released; then
  snapshot_resources after-release-timeout
  run_capture errors-after-release-timeout \
    cargo run --quiet -p squidc -- device errors >/dev/null
  exit 1
fi
snapshot_resources after-release

printf '%s\n' \
  "Press and hold ${INPUT_BUTTON_LABEL}, or short GPIO9 to GND, until this script asks you to release it." >&2
if ! wait_for_input_pressed; then
  snapshot_resources after-press-timeout
  run_capture errors-after-press-timeout \
    cargo run --quiet -p squidc -- device errors >/dev/null
  exit 1
fi
snapshot_resources after-press-observed

if ! wait_for_contains_or_timeout input-output-press "output=count 1" \
  "device output" cargo run --quiet -p squidc -- device output >/dev/null; then
  snapshot_resources after-dispatch-timeout
  run_capture errors-after-dispatch-timeout \
    cargo run --quiet -p squidc -- device errors >/dev/null
  exit 1
fi

printf 'Release %s now.\n' "${INPUT_BUTTON_LABEL}" >&2
if ! wait_for_input_released; then
  snapshot_resources after-final-release-timeout
  run_capture errors-after-final-release-timeout \
    cargo run --quiet -p squidc -- device errors >/dev/null
  exit 1
fi
snapshot_resources after-press

printf 'OK ESP32-C3 input stack isolation resources captured: %s\n' "${summary_out}"
