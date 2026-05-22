#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
APP_DIR="${ROOT}/firmware/zephyr"
SUPERMINI_OVERLAY="${APP_DIR}/boards/esp32c3_supermini.overlay"
EXTRA_ARGS=("$@")

if ! command -v west >/dev/null 2>&1; then
  printf 'west is required for Zephyr firmware builds. Install Zephyr SDK/tools and run west init/update first.\n' >&2
  exit 1
fi

printf 'Using Zephyr board %s (unverified default for ESP32-C3 Super Mini; override with ZEPHYR_BOARD).\n' "$ZEPHYR_BOARD" >&2

west build \
  --build-dir "$ZEPHYR_BUILD_DIR" \
  --board "$ZEPHYR_BOARD" \
  "$APP_DIR" \
  "${EXTRA_ARGS[@]}" \
  -- \
  -DDTC_OVERLAY_FILE="$SUPERMINI_OVERLAY"

printf '%s\n' "$ZEPHYR_BUILD_DIR"
