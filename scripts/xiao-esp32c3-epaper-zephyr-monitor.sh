#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

PORT="$(resolve_esp_serial_port)"

if ! command -v west >/dev/null 2>&1; then
  printf 'west is required for Zephyr firmware monitoring.\n' >&2
  exit 1
fi

cd "$ZEPHYR_BUILD_DIR"
west espressif monitor -p "$PORT" -e zephyr/zephyr.elf "$@"
