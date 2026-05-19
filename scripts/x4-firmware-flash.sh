#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_38:44:BE:98:72:DC-if00}"
ELF="${ROOT}/firmware/squid-firmware/target/riscv32imc-unknown-none-elf/release/x4-hello"

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
  printf 'espflash is required for flashing. Install with: cargo install espflash --locked\n' >&2
  exit 1
fi

"${ROOT}/scripts/x4-firmware-backup.sh"
"${ROOT}/scripts/x4-firmware-build.sh" >/dev/null

"$ESPFLASH_BIN" flash \
  --chip esp32c3 \
  --port "$PORT" \
  --before usb-reset \
  --flash-size 16mb \
  --monitor \
  "$ELF"
