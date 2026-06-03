#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_supermini}"
export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
export SQUID_ZEPHYR_FALLBACK_SOURCE="${SQUID_ZEPHYR_FALLBACK_SOURCE:-${ROOT}/firmware/zephyr/fallback/esp32c3-supermini-main.squid}"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

SKIP_FLASH="${SKIP_FLASH:-0}"
BANNER="${SQUID_ZEPHYR_DIAGNOSTIC_BANNER:-SquidScript Zephyr firmware diagnostic boot}"
BANNER_TIMEOUT_SECONDS="${SQUID_ZEPHYR_DIAGNOSTIC_BANNER_TIMEOUT_SECONDS:-12}"
LOG_DIR="${SQUID_ZEPHYR_DIAGNOSTIC_LOG_DIR:-${ROOT}/target/hardware-tests/diagnostic}"
PORT="$(resolve_esp_serial_port)"

if [[ "$SKIP_FLASH" != "1" ]]; then
  cargo run --quiet -p squidc -- target flash --target esp32c3-super-mini
else
  cargo run --quiet -p squidc -- target build --target esp32c3-super-mini >/dev/null
fi

printf '%s\n' 'OK zephyr diagnostic image build/flash step completed'

mkdir -p "$LOG_DIR"
monitor_log="${LOG_DIR}/boot-banner.log"
monitor_cmd=(cargo run --quiet -p squidc -- target monitor --target esp32c3-super-mini --port "$PORT")
monitor_shell_command="$(printf '%q ' "${monitor_cmd[@]}")"

set +e
if command -v script >/dev/null 2>&1; then
  (
    cd "$ROOT"
    timeout "${BANNER_TIMEOUT_SECONDS}s" script -q -e -c "$monitor_shell_command" /dev/null
  ) >"$monitor_log" 2>&1
else
  (
    cd "$ROOT"
    timeout "${BANNER_TIMEOUT_SECONDS}s" "${monitor_cmd[@]}"
  ) >"$monitor_log" 2>&1
fi
monitor_status=$?
set -e

if ! grep -Fq "$BANNER" "$monitor_log"; then
  printf 'Expected diagnostic boot banner not found within %ss on %s.\n' \
    "$BANNER_TIMEOUT_SECONDS" "$PORT" >&2
  printf 'Expected: %s\n' "$BANNER" >&2
  printf 'Monitor log: %s\n' "$monitor_log" >&2
  exit 1
fi

if [[ "$monitor_status" != "0" && "$monitor_status" != "124" ]]; then
  printf 'Diagnostic monitor exited with status %s after banner capture.\n' \
    "$monitor_status" >&2
  printf 'Monitor log: %s\n' "$monitor_log" >&2
  exit "$monitor_status"
fi

printf 'OK diagnostic boot banner verified on %s\n' "$PORT"
printf 'diagnostic monitor log: %s\n' "$monitor_log"
