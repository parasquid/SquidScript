#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-}"
ELF="${ROOT}/firmware/squid-firmware/target/riscv32imc-unknown-none-elf/release/c3-supermini-serial-hello"
EXTRA_ARGS=("$@")
MONITOR_AFTER_FLASH="${MONITOR_AFTER_FLASH:-0}"

if command -v espflash >/dev/null 2>&1; then
  ESPFLASH_BIN="$(command -v espflash)"
elif [[ -x "$HOME/.cargo/bin/espflash" ]]; then
  ESPFLASH_BIN="$HOME/.cargo/bin/espflash"
else
  printf 'espflash is required for flashing. Install with: cargo install espflash --locked\n' >&2
  exit 1
fi

"${ROOT}/scripts/c3-supermini-build.sh" >/dev/null

if [[ -n "$PORT" ]]; then
  FLASH_ARGS=(
    --chip esp32c3 \
    --port "$PORT" \
    --before usb-reset \
    --after hard-reset \
    --flash-size 4mb
  )
else
  FLASH_ARGS=(
    --chip esp32c3 \
    --before usb-reset \
    --after hard-reset \
    --flash-size 4mb
  )
fi

if [[ "$MONITOR_AFTER_FLASH" == "1" ]]; then
  FLASH_ARGS+=(--monitor --monitor-baud 115200)
fi

"$ESPFLASH_BIN" flash "${FLASH_ARGS[@]}" "${EXTRA_ARGS[@]}" "$ELF"
