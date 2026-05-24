#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/serial-port.sh"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-hardware.sh [--skip-flash]

Runs the current Zephyr-backed ESP32-C3 Super Mini hardware checks
sequentially. Stateful install/lifecycle checks run before any final visible
board-state checks.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      export SKIP_FLASH=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

export ESPFLASH_PORT="$(resolve_esp_serial_port)"

"$ROOT/scripts/c3-supermini-zephyr-test-diagnostic.sh"
"$ROOT/scripts/c3-supermini-test-app-state.sh"
"$ROOT/scripts/c3-supermini-test-foreground-memory.sh"
"$ROOT/scripts/c3-supermini-test-app-lifecycle.sh"
"$ROOT/scripts/c3-supermini-test-display-drawlog.sh"
"$ROOT/scripts/c3-supermini-test-system-resources.sh"
"$ROOT/scripts/c3-supermini-test-device-binding.sh"
"$ROOT/scripts/c3-supermini-test-inline-gpio-binding.sh"
"$ROOT/scripts/c3-supermini-test-device-config.sh"
"$ROOT/scripts/c3-supermini-measure-stack-usage.sh"
"$ROOT/scripts/c3-supermini-test-wifi-state.sh" --require-real-wifi
"$ROOT/scripts/c3-supermini-test-wifi-scan-api.sh" --require-real-wifi
"$ROOT/scripts/c3-supermini-test-wifi-list-api.sh" --require-real-wifi
"$ROOT/scripts/c3-supermini-test-wifi-ap-api.sh"
"$ROOT/scripts/c3-supermini-test-blinky.sh"
