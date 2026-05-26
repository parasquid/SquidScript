#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
MODE="${MODE:-representative}"
WORK_DIR="${ROOT}/target/hardware-benchmarks/lazy-load-screen-${MODE}"
case "${MODE}" in
  representative)
    APP="${ROOT}/tests/hardware/c3-supermini/lazy-load-screen-benchmark/main.squid"
    APP_ID="lazy-load-screen-benchmark"
    ;;
  worst)
    APP="${ROOT}/tests/hardware/c3-supermini/lazy-load-screen-worst-case/main.squid"
    APP_ID="lazy-load-screen-worst-case"
    ;;
  *)
    printf 'Unsupported MODE=%s; expected representative or worst\n' "${MODE}" >&2
    exit 2
    ;;
esac
TRANSITIONS="${TRANSITIONS:-30}"

mkdir -p "${WORK_DIR}"


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

resource_value() {
  local file="$1"
  local key="$2"
  awk -F= -v key="$key" '$1 == key { print $2; found = 1 } END { if (!found) exit 1 }' "${file}"
}

wait_for_dispatch_after() {
  local label="$1"
  local previous="$2"
  local out="${WORK_DIR}/${label}.out"
  local sequence

  for _ in $(seq 1 120); do
    timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" cargo run --quiet -p squidc -- device resources >"${out}" 2>&1
    sequence="$(resource_value "${out}" "last_dispatch_sequence")"
    if ((sequence > previous)); then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.05
  done

  printf 'Timed out waiting for dispatch sequence greater than %s\n' "${previous}" >&2
  printf '%s\n' "--- ${out} ---" >&2
  sed -n '1,200p' "${out}" >&2
  exit 1
}

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-benchmark cargo run --quiet -p squidc -- app install "${APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=${APP_ID}"

run_capture launch-benchmark cargo run --quiet -p squidc -- app launch "${APP_ID}" >/dev/null
post_launch_errors_out="$(run_capture post-launch-errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${post_launch_errors_out}"
baseline_out="$(run_capture baseline-resources cargo run --quiet -p squidc -- device resources)"
last_sequence="$(resource_value "${baseline_out}" "last_dispatch_sequence")"

elapsed_values=()
read_count_total=0
read_bytes_total=0

for i in $(seq 1 "${TRANSITIONS}"); do
  resources_out="$(wait_for_dispatch_after "resources-${i}" "${last_sequence}")"
  last_sequence="$(resource_value "${resources_out}" "last_dispatch_sequence")"
  elapsed="$(resource_value "${resources_out}" "last_dispatch_elapsed_us")"
  reads="$(resource_value "${resources_out}" "last_dispatch_sqbc_read_count")"
  bytes="$(resource_value "${resources_out}" "last_dispatch_sqbc_read_bytes")"
  elapsed_values+=("${elapsed}")
  read_count_total=$((read_count_total + reads))
  read_bytes_total=$((read_bytes_total + bytes))
done

mapfile -t sorted_elapsed < <(printf '%s\n' "${elapsed_values[@]}" | sort -n)
count="${#sorted_elapsed[@]}"
median_index=$((count / 2))
p95_index=$((((count * 95) + 99) / 100 - 1))
if ((p95_index < 0)); then
  p95_index=0
fi

summary_out="${WORK_DIR}/summary.out"
{
  printf 'benchmark=lazy_load_screen_transition\n'
  printf 'mode=%s\n' "${MODE}"
  printf 'target=ESP32-C3 Super Mini\n'
  printf 'transition_count=%s\n' "${count}"
  printf 'dispatch_elapsed_us_min=%s\n' "${sorted_elapsed[0]}"
  printf 'dispatch_elapsed_us_median=%s\n' "${sorted_elapsed[median_index]}"
  printf 'dispatch_elapsed_us_p95=%s\n' "${sorted_elapsed[p95_index]}"
  printf 'dispatch_elapsed_us_max=%s\n' "${sorted_elapsed[count - 1]}"
  printf 'sqbc_read_count_total=%s\n' "${read_count_total}"
  printf 'sqbc_read_bytes_total=%s\n' "${read_bytes_total}"
} >"${summary_out}"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

cat "${summary_out}"
