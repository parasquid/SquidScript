#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_supermini}"
export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
export SQUID_ZEPHYR_FALLBACK_SOURCE="${SQUID_ZEPHYR_FALLBACK_SOURCE:-${ROOT}/firmware/zephyr/fallback/esp32c3-supermini-main.squid}"
source "${ROOT}/scripts/zephyr-env.sh"
EXTRA_ARGS=("$@")
MONITOR_AFTER_FLASH="${MONITOR_AFTER_FLASH:-0}"

if ! command -v west >/dev/null 2>&1; then
  printf 'west is required for Zephyr firmware flashing.\n' >&2
  exit 1
fi

"${ROOT}/scripts/c3-supermini-zephyr-build.sh" >/dev/null
west flash --build-dir "$ZEPHYR_BUILD_DIR" "${EXTRA_ARGS[@]}"

if [[ "$MONITOR_AFTER_FLASH" == "1" ]]; then
  "$ROOT/scripts/c3-supermini-zephyr-monitor.sh"
fi
