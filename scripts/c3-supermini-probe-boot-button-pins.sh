#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"

WORK_DIR="${ROOT}/target/hardware-tests/boot-button-pin-scan"
PIN_SCAN_APP="${ROOT}/tests/hardware/c3-supermini/boot-button-pin-scan/main.squid"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-12}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-60}"
STABLE_SAMPLE_COUNT="${STABLE_SAMPLE_COUNT:-3}"

mkdir -p "${WORK_DIR}"

latest_pin_sample() {
  local file="$1"
  awk '
    /^output=pin / {
      lines[++count] = $0
    }
    END {
      if (count < 11) {
        exit 1
      }
      for (i = count - 10; i <= count; i++) {
        print lines[i]
      }
    }
  ' "$file"
}

capture_sample() {
  local label="$1"
  run_capture "${label}-launch" \
    cargo run --quiet -p squidc -- app launch boot-button-pin-scan >/dev/null
  run_capture "${label}-output" cargo run --quiet -p squidc -- device output
}

wait_for_changed_sample() {
  local baseline="$1"
  local out sample
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local attempt=0
  local stable_count=0
  local last_changed_sample=""

  while (( SECONDS < deadline )); do
    attempt=$((attempt + 1))
    out="$(capture_sample "held-${attempt}")"
    sample="$(latest_pin_sample "${out}")"
    if [[ "${sample}" != "${baseline}" ]]; then
      if [[ "${sample}" == "${last_changed_sample}" ]]; then
        stable_count=$((stable_count + 1))
      else
        last_changed_sample="${sample}"
        stable_count=1
      fi
      if (( stable_count >= STABLE_SAMPLE_COUNT )); then
        printf '%s\n' "${sample}"
        return 0
      fi
    else
      stable_count=0
      last_changed_sample=""
    fi
    sleep 0.1
  done

  printf 'Timed out waiting for %s stable BOOT-button pin samples changed from baseline:\n%s\n' \
    "${STABLE_SAMPLE_COUNT}" "${baseline}" >&2
  if [[ -n "${last_changed_sample}" ]]; then
    printf 'Last unstable changed sample:\n%s\n' "${last_changed_sample}" >&2
  fi
  if [[ -n "${out:-}" ]]; then
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
  fi
  run_capture errors-after-timeout cargo run --quiet -p squidc -- device errors >/dev/null
  exit 1
}

run_capture install-pin-scan cargo run --quiet -p squidc -- app install "${PIN_SCAN_APP}" >/dev/null

printf '%s\n' 'Release the ESP32-C3 Super Mini BOOT button now.' >&2
run_capture reset-before-released cargo run --quiet -p squidc -- device reset >/dev/null
released_out="$(capture_sample released)"
baseline="$(latest_pin_sample "${released_out}")"
printf 'released: %s\n' "${baseline}"

printf '%s\n' 'Keep the ESP32-C3 Super Mini BOOT button released for reset.' >&2
run_capture reset-before-held cargo run --quiet -p squidc -- device reset >/dev/null
printf '%s\n' 'Press and hold the ESP32-C3 Super Mini BOOT button now.' >&2
held="$(wait_for_changed_sample "${baseline}")"
printf 'held: %s\n' "${held}"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
if [[ -s "${errors_out}" ]]; then
  printf 'Expected %s to be empty\n' "${errors_out}" >&2
  printf '%s\n' "--- ${errors_out} ---" >&2
  sed -n '1,200p' "${errors_out}" >&2
  exit 1
fi

printf '%s\n' 'OK ESP32-C3 BOOT button pin scan observed a changed GPIO sample'
