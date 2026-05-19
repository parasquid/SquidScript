#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_38:44:BE:98:72:DC-if00}"
OUT_DIR="${ROOT}/target/device-backups"
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="${OUT_DIR}/xteink-x4-flash-${STAMP}.bin"
SHA="${BACKUP}.sha256"

mkdir -p "$OUT_DIR"

if [[ ! -e "$PORT" ]]; then
  printf 'Serial port not found: %s\n' "$PORT" >&2
  printf 'Set ESPFLASH_PORT=/path/to/device if the X4 enumerated differently.\n' >&2
  exit 1
fi

if ! command -v esptool >/dev/null 2>&1; then
  printf 'esptool is required for flash backup.\n' >&2
  exit 1
fi

esptool --chip esp32c3 --port "$PORT" --baud 460800 read-flash 0 16MB "$BACKUP"
sha256sum "$BACKUP" | tee "$SHA"

printf 'Wrote backup: %s\n' "$BACKUP"
