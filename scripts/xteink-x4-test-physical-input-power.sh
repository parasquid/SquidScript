#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

PORT="${PORT:-}"
CHECK_ONLY=0
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-physical-input-power}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-physical-input-power.sh [--port <port>]
       scripts/xteink-x4-test-physical-input-power.sh --check

Interactive final parity gate. A person must press the real X4 buttons only
when prompted. The script verifies all ADC keys, POWER short/long/double
classification, armed app routing, timerless sleep, physical POWER wake,
serial responsiveness, lifecycle state, and captures the redraw panel.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) PORT="${2:-}"; shift 2 ;;
    --check) CHECK_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
mkdir -p "${WORK_DIR}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [[ "${CHECK_ONLY}" == 1 ]]; then
  cargo run --quiet -p squidc -- app package \
    "${ROOT}/tests/hardware/xteink-x4/key-detector" --target xteink-x4 \
    --out "${WORK_DIR}/key-detector.squid.zip" >/dev/null
  for app in main redraw-helper sleep-helper; do
    cargo run --quiet -p squidc -- app build \
      "${ROOT}/examples/power-gesture-redraw/${app}.squid" --target xteink-x4 \
      --out "${WORK_DIR}/${app}.sqbc" >/dev/null
  done
  printf 'OK XTEINK X4 physical input/power runner fixtures compile\n'
  exit 0
fi
[[ -t 0 ]] || { printf 'Physical input test requires an interactive terminal\n' >&2; exit 2; }
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi

wait_for_device() {
  local deadline=$((SECONDS + 45))
  while (( SECONDS < deadline )); do
    if cargo run --quiet -p squidc -- device firmware-info --port "${PORT}" \
      >"${WORK_DIR}/device-ready.out" 2>&1; then
      return 0
    fi
    sleep 1
  done
  printf 'X4 did not return after physical POWER wake\n' >&2
  return 1
}

prompt_event() {
  local label="$1"
  local expected="$2"
  expected_count=$((expected_count + 1))
  printf '\nPerform physical action: %s\nPress Enter only after releasing the button.\n' "${label}"
  read -r
  local output
  output="$(run_capture "output-${expected//./-}" cargo run --quiet -p squidc -- device output --port "${PORT}")"
  grep -Fq "output=key ${expected} ${expected_count}" "${output}"
  run_capture "resources-${expected//./-}" cargo run --quiet -p squidc -- device resources --port "${PORT}" >/dev/null
}

run_capture format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture package-keys cargo run --quiet -p squidc -- app package \
  "${ROOT}/tests/hardware/xteink-x4/key-detector" --target xteink-x4 \
  --out "${WORK_DIR}/key-detector.squid.zip" >/dev/null
run_capture install-keys cargo run --quiet -p squidc -- app install \
  "${WORK_DIR}/key-detector.squid.zip" --port "${PORT}" >/dev/null
run_capture launch-keys cargo run --quiet -p squidc -- app launch key-detector --port "${PORT}" >/dev/null

expected_count=0
prompt_event UP UP
prompt_event DOWN DOWN
prompt_event LEFT LEFT
prompt_event RIGHT RIGHT
prompt_event SELECT SELECT
prompt_event BACK BACK
prompt_event 'single POWER tap; wait at least 400 ms before Enter' POWER
prompt_event 'hold POWER for at least 500 ms' POWER.longTap
prompt_event 'double-tap POWER with less than 350 ms between taps' POWER.doubleTap

run_capture format-gestures cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
for app in main redraw-helper sleep-helper; do
  run_capture "install-${app}" cargo run --quiet -p squidc -- app install \
    "${ROOT}/examples/power-gesture-redraw/${app}.squid" --port "${PORT}" >/dev/null
done
run_capture launch-main cargo run --quiet -p squidc -- app launch main --port "${PORT}" >/dev/null
run_capture initial-main-output cargo run --quiet -p squidc -- device output --port "${PORT}" >/dev/null
printf '\nDouble-tap physical POWER now, then wait for the redraw and press Enter.\n'
read -r
lifecycle="$(run_capture lifecycle-double cargo run --quiet -p squidc -- device lifecycle --port "${PORT}")"
grep -Fq 'active=main' "${lifecycle}"
output="$(run_capture output-double cargo run --quiet -p squidc -- device output --port "${PORT}")"
grep -Fq 'output=redraw-helper' "${output}"
grep -Fq 'output=redraw 2' "${output}"
ffmpeg -hide_banner -loglevel error -f video4linux2 -video_size 1920x1080 \
  -i /dev/video1 -frames:v 1 "${WORK_DIR}/double-tap-redraw.jpg"

printf '\nHold physical POWER for at least 500 ms. The app should enter timerless sleep.\n'
printf 'After USB disconnects, press physical POWER once to wake it, wait for reconnect, then press Enter.\n'
read -r
wait_for_device
lifecycle="$(run_capture lifecycle-wake cargo run --quiet -p squidc -- device lifecycle --port "${PORT}")"
grep -Fq 'start_reason=wake' "${lifecycle}"
grep -Fq 'process_stack[0]=main' "${lifecycle}"
output="$(run_capture output-wake cargo run --quiet -p squidc -- device output --port "${PORT}")"
grep -Fq 'output=sleep-helper' "${output}"
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 physical input, gesture routing, timerless sleep, and POWER wake\n'
printf 'Panel evidence: %s\n' "${WORK_DIR}/double-tap-redraw.jpg"
