#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-}"
OUT_DIR="${ROOT}/target/device-backups"
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="${OUT_DIR}/xteink-x4-flash-${STAMP}.bin"
SHA="${BACKUP}.sha256"

mkdir -p "$OUT_DIR"

if [[ -z "$PORT" ]]; then
  mapfile -t CANDIDATES < <(find /dev/serial/by-id -maxdepth 1 -type l -name 'usb-Espressif_USB_JTAG_serial_debug_unit_*-if00' 2>/dev/null | sort)
  if [[ "${#CANDIDATES[@]}" == "1" ]]; then
    PORT="${CANDIDATES[0]}"
  fi
fi

if [[ ! -e "$PORT" ]]; then
  printf 'X4 serial port not found.\n' >&2
  printf 'Set ESPFLASH_PORT=/path/to/device or connect exactly one Espressif USB JTAG serial device.\n' >&2
  exit 1
fi

if ! command -v esptool >/dev/null 2>&1; then
  printf 'esptool is required for flash backup.\n' >&2
  exit 1
fi

esptool --chip esp32c3 --port "$PORT" --baud 460800 read-flash 0 16MB "$BACKUP"
sha256sum "$BACKUP" | tee "$SHA"

printf 'Wrote backup: %s\n' "$BACKUP"
