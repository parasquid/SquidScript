#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-/dev/ttyACM0}"
SOURCE="${1:-$ROOT/compiler/rust/fixtures/conformance/headless_counter.squid}"
OUT="${2:-$ROOT/target/reference-firmware/headless_counter.sqbc}"

mkdir -p "$(dirname "$OUT")"
"$ROOT/scripts/squidc-build.sh" build "$SOURCE" --target esp32c3-super-mini --out "$OUT"

PYTHONPATH="$ROOT/scripts" python3 "$ROOT/scripts/c3_supermini_serial.py" \
  --port "$PORT" \
  install "$OUT"
