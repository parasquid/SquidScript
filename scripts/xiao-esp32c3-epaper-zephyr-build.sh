#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
APP_DIR="${ROOT}/firmware/zephyr"
EXTRA_ARGS=("$@")
TARGET_KCONFIG="${ROOT}/target/zephyr/generated/xiao-esp32c3-gdeq0426t82-sd-target.conf"
mkdir -p "$(dirname "$TARGET_KCONFIG")"
"${ROOT}/scripts/generate-zephyr-target-kconfig.py" "$SQUID_ZEPHYR_TARGET_JSON" "$TARGET_KCONFIG"

CMAKE_ARGS=(
  -DDTC_OVERLAY_FILE="$SQUID_ZEPHYR_TARGET_OVERLAY"
  -DSQUID_ZEPHYR_TARGET_JSON="$SQUID_ZEPHYR_TARGET_JSON"
  -DSQUID_ZEPHYR_TARGET_OVERLAY="$SQUID_ZEPHYR_TARGET_OVERLAY"
  -DSQUID_ZEPHYR_FALLBACK_SOURCE="$SQUID_ZEPHYR_FALLBACK_SOURCE"
)
EXTRA_CONF_FILES=("$TARGET_KCONFIG")
ZEPHYR_PRISTINE="${ZEPHYR_PRISTINE:-auto}"

if [[ -n "${ZEPHYR_EXTRA_CONF_FILE:-}" ]]; then
  EXTRA_CONF_FILES+=("${ZEPHYR_EXTRA_CONF_FILE}")
fi
CMAKE_ARGS+=(-DEXTRA_CONF_FILE="$(IFS=';'; printf '%s' "${EXTRA_CONF_FILES[*]}")")

if [[ "${SQUID_ZEPHYR_STACK_USAGE:-0}" == "1" ]]; then
  CMAKE_ARGS+=(-DSQUID_ZEPHYR_STACK_USAGE=ON)
fi

if ! command -v west >/dev/null 2>&1; then
  printf 'west is required for Zephyr firmware builds. Install Zephyr SDK/tools and run west init/update first.\n' >&2
  exit 1
fi

printf 'Using Zephyr board %s for XIAO ESP32-C3 e-paper target.\n' "$ZEPHYR_BOARD" >&2

west build \
  --build-dir "$ZEPHYR_BUILD_DIR" \
  --board "$ZEPHYR_BOARD" \
  --pristine "$ZEPHYR_PRISTINE" \
  "$APP_DIR" \
  "${EXTRA_ARGS[@]}" \
  -- \
  "${CMAKE_ARGS[@]}"

printf '%s\n' "$ZEPHYR_BUILD_DIR"
