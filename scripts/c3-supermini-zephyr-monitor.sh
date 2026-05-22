#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"

if ! command -v west >/dev/null 2>&1; then
  printf 'west is required for Zephyr firmware monitoring.\n' >&2
  exit 1
fi

west espressif monitor --build-dir "$ZEPHYR_BUILD_DIR" "$@"
