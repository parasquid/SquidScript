#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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
  west espressif monitor --build-dir "$ZEPHYR_BUILD_DIR"
fi
