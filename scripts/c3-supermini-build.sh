#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIRMWARE_DIR="${ROOT}/firmware/squid-firmware"
RUSTC_PATH="${RUSTC:-$(rustup which rustc)}"
BUILD_ID="${SQUID_FIRMWARE_BUILD_ID:-$(git -C "$ROOT" rev-parse --short=12 HEAD 2>/dev/null || date +%Y%m%d%H%M)}"

cd "$FIRMWARE_DIR"

SQUID_FIRMWARE_BUILD_ID="$BUILD_ID" \
RUSTC="$RUSTC_PATH" \
rustup run stable cargo build --release --features hardware --bin c3-supermini-serial-hello

printf '%s\n' "$FIRMWARE_DIR/target/riscv32imc-unknown-none-elf/release/c3-supermini-serial-hello"
