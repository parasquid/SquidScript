#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
EXPERIMENT_DIR="${ROOT}/experiments/esp32c3-supermini/firmware/embassy-wifi-ap-probe"
PARTITION_TABLE="${ROOT}/firmware/squid-firmware/partitions/c3-supermini.csv"
ELF="${EXPERIMENT_DIR}/target/riscv32imc-unknown-none-elf/release/embassy-wifi-ap-probe"
PORT="${ESPFLASH_PORT:-}"

"${EXPERIMENT_DIR}/build.sh" >/dev/null

if command -v espflash >/dev/null 2>&1; then
  ESPFLASH_BIN="$(command -v espflash)"
elif [[ -x "$HOME/.cargo/bin/espflash" ]]; then
  ESPFLASH_BIN="$HOME/.cargo/bin/espflash"
else
  printf 'espflash is required for flashing. Install with: cargo install espflash --locked\n' >&2
  exit 1
fi

if [[ -n "$PORT" ]]; then
  PORT_ARGS=(--port "$PORT")
else
  PORT_ARGS=()
fi

"$ESPFLASH_BIN" flash \
  --chip esp32c3 \
  "${PORT_ARGS[@]}" \
  --before usb-reset \
  --after hard-reset \
  --flash-size 4mb \
  --partition-table "$PARTITION_TABLE" \
  --target-app-partition factory \
  "$ELF"
