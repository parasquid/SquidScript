#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/serial-port.sh"

SKIP_PHYSICAL_INPUT=0

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-hardware-non-scan.sh [--skip-flash] [--skip-physical-input]

Runs the current Zephyr-backed ESP32-C3 Super Mini hardware checks
sequentially, excluding Wi-Fi scan/list coverage. Use this when scan/list is
verified separately but same-build RAM/stack evidence is still needed. The
final visible blinky check runs last.

Options:
  --skip-flash             Reuse the firmware already on the device.
  --skip-physical-input    Skip the BOOT/GPIO9 press prompt.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      export SKIP_FLASH=1
      shift
      ;;
    --skip-physical-input)
      SKIP_PHYSICAL_INPUT=1
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
"$ROOT/scripts/c3-supermini-test-app-registry-api.sh"
"$ROOT/scripts/c3-supermini-test-display-drawlog.sh"
"$ROOT/scripts/c3-supermini-test-system-resources.sh"
"$ROOT/scripts/c3-supermini-test-indicator-state.sh"
"$ROOT/scripts/c3-supermini-test-device-binding.sh"
"$ROOT/scripts/c3-supermini-test-inline-gpio-binding.sh"
"$ROOT/scripts/c3-supermini-test-inline-gpio10-binding.sh"
if [[ "$SKIP_PHYSICAL_INPUT" != "1" ]]; then
  "$ROOT/scripts/c3-supermini-test-input-button.sh"
fi
"$ROOT/scripts/c3-supermini-test-unsupported-inline-gpio-binding.sh"
"$ROOT/scripts/c3-supermini-test-device-config.sh"
"$ROOT/scripts/c3-supermini-test-file-pick.sh"
"$ROOT/scripts/c3-supermini-measure-stack-usage.sh"
"$ROOT/scripts/c3-supermini-test-wifi-state.sh" --require-real-wifi
"$ROOT/scripts/c3-supermini-test-wifi-ap-api.sh"
"$ROOT/scripts/c3-supermini-test-blinky.sh"
