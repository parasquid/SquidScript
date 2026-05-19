#!/usr/bin/env bash
set -euo pipefail

PORT="${ESPFLASH_PORT:-/dev/ttyACM0}"

if [[ ! -e "$PORT" ]]; then
  printf 'Serial port not found: %s\n' "$PORT" >&2
  printf 'Set ESPFLASH_PORT=/path/to/device if the Super Mini enumerated differently.\n' >&2
  exit 1
fi

if command -v espflash >/dev/null 2>&1; then
  ESPFLASH_BIN="$(command -v espflash)"
elif [[ -x "$HOME/.cargo/bin/espflash" ]]; then
  ESPFLASH_BIN="$HOME/.cargo/bin/espflash"
else
  printf 'espflash is required for monitoring. Install with: cargo install espflash --locked\n' >&2
  exit 1
fi

"$ESPFLASH_BIN" monitor --chip esp32c3 --port "$PORT" --before no-reset
