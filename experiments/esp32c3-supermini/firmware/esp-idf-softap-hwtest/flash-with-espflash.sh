#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PORT="${1:-/dev/ttyACM0}"
ESPFLASH="${ESPFLASH:-$HOME/.cargo/bin/espflash}"

if [[ ! -x "$ESPFLASH" ]]; then
  echo "espflash not found at $ESPFLASH" >&2
  exit 1
fi

for file in \
  build/bootloader/bootloader.bin \
  build/esp32c3_softap_hwtest.elf \
  partitions.csv; do
  if [[ ! -f "$file" ]]; then
    echo "missing build output: $file" >&2
    echo "run ./build.sh first" >&2
    exit 1
  fi
done

"$ESPFLASH" flash \
  --chip esp32c3 \
  --port "$PORT" \
  --before usb-reset \
  --after hard-reset \
  --non-interactive \
  --flash-mode dio \
  --flash-freq 80mhz \
  --flash-size 4mb \
  --bootloader build/bootloader/bootloader.bin \
  --partition-table partitions.csv \
  build/esp32c3_softap_hwtest.elf
