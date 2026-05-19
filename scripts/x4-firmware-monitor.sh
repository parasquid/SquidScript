#!/usr/bin/env bash
set -euo pipefail

PORT="${ESPFLASH_PORT:-/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_38:44:BE:98:72:DC-if00}"

if [[ ! -e "$PORT" ]]; then
  printf 'Serial port not found: %s\n' "$PORT" >&2
  printf 'Set ESPFLASH_PORT=/path/to/device if the X4 enumerated differently.\n' >&2
  exit 1
fi

if command -v espflash >/dev/null 2>&1; then
  ESPFLASH_BIN="$(command -v espflash)"
elif [[ -x "$HOME/.cargo/bin/espflash" ]]; then
  ESPFLASH_BIN="$HOME/.cargo/bin/espflash"
else
  ESPFLASH_BIN=""
fi

if [[ -n "$ESPFLASH_BIN" ]]; then
  "$ESPFLASH_BIN" monitor --chip esp32c3 --port "$PORT" --before usb-reset
elif python3 -c 'import serial' >/dev/null 2>&1; then
  python3 -m serial.tools.miniterm "$PORT" 115200
else
  printf 'Need espflash or pyserial for monitoring.\n' >&2
  printf 'Install espflash with: cargo install espflash --locked\n' >&2
  exit 1
fi
