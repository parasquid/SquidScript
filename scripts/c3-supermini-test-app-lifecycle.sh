#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/serial-port.sh"

export ESPFLASH_PORT="$(resolve_esp_serial_port)"

WORK_DIR="${ROOT}/target/hardware-tests/app-lifecycle"
MAIN_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/main.squid"
BREAK_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/break-reminder.squid"
READER_APP="${ROOT}/tests/hardware/c3-supermini/generic-events/reader-clock.squid"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware app lifecycle: %s\n' "$*" >&2
  "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_json_lifecycle() {
  local file="$1"
  local expected_active="$2"
  local expected_process="$3"
  local expected_armed="$4"

  python -c '
import json
import sys

path, expected_active, expected_process, expected_armed = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)["data"]
process = ",".join(data["processStack"])
armed = ",".join("{}:{}".format(entry["appId"], entry["event"]) for entry in data["armedStack"])
if data["active"] != expected_active or process != expected_process or armed != expected_armed:
    print(f"Expected active={expected_active} process={expected_process} armed={expected_armed}", file=sys.stderr)
    print(json.dumps(data, indent=2), file=sys.stderr)
    sys.exit(1)
' "${file}" "${expected_active}" "${expected_process}" "${expected_armed}"
}

wait_for_contains() {
  local label="$1"
  local expected="$2"
  local command_name="$3"
  shift 3
  local out="${WORK_DIR}/${label}.out"

  for _ in $(seq 1 40); do
    "$@" >"${out}" 2>&1
    if grep -Fq "${expected}" "${out}"; then
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

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-main cargo run --quiet -p squidc -- app install "${MAIN_APP}" >/dev/null
run_capture install-break cargo run --quiet -p squidc -- app install "${BREAK_APP}" >/dev/null
run_capture install-reader cargo run --quiet -p squidc -- app install "${READER_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=main"
assert_file_contains "${apps_out}" "app=break-reminder"
assert_file_contains "${apps_out}" "app=reader-clock"

run_capture launch-main cargo run --quiet -p squidc -- app launch main >/dev/null

lifecycle_out="$(wait_for_contains lifecycle-reader "lifecycle=active=reader-clock" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"
assert_file_contains "${lifecycle_out}" "lifecycle=armed_stack[0]=break-reminder timer.break"
json_lifecycle_out="$(run_capture lifecycle-reader-json cargo run --quiet -p squidc -- --json device lifecycle)"
assert_json_lifecycle "${json_lifecycle_out}" "reader-clock" "main" "break-reminder:timer.break"

output_out="$(wait_for_contains output-reader "output=reader start" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=main start"

lifecycle_out="$(wait_for_contains lifecycle-break "lifecycle=active=break-reminder" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[1]=reader-clock"
json_lifecycle_out="$(run_capture lifecycle-break-json cargo run --quiet -p squidc -- --json device lifecycle)"
assert_json_lifecycle "${json_lifecycle_out}" "break-reminder" "main,reader-clock" ""

output_out="$(wait_for_contains output-break "output=break fired" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=reader start"

run_capture exit-break cargo run --quiet -p squidc -- device key SELECT >/dev/null

lifecycle_out="$(wait_for_contains lifecycle-return "lifecycle=active=reader-clock" \
  "device lifecycle" cargo run --quiet -p squidc -- device lifecycle)"
assert_file_contains "${lifecycle_out}" "lifecycle=process_stack[0]=main"
json_lifecycle_out="$(run_capture lifecycle-return-json cargo run --quiet -p squidc -- --json device lifecycle)"
assert_json_lifecycle "${json_lifecycle_out}" "reader-clock" "main" ""

output_out="$(wait_for_contains output-return "output=break exit" \
  "device output" cargo run --quiet -p squidc -- device output)"
assert_file_contains "${output_out}" "output=reader start"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr app lifecycle SquidScript hardware check passed'
