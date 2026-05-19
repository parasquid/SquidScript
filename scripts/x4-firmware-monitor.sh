#!/usr/bin/env bash
set -euo pipefail

PORT="${ESPFLASH_PORT:-}"

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
