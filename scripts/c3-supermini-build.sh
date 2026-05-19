#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIRMWARE_DIR="${ROOT}/firmware/squid-firmware"
RUSTC_PATH="${RUSTC:-$(rustup which rustc)}"
BUILD_ID="${SQUID_FIRMWARE_BUILD_ID:-$(git -C "$ROOT" rev-parse --short=12 HEAD 2>/dev/null || date +%Y%m%d%H%M)}"
FREESTANDING_INCLUDE="${FIRMWARE_DIR}/c-freestanding/include"

if [[ -z "${CC_riscv32imc_unknown_none_elf:-}" ]]; then
  if command -v riscv32-unknown-elf-gcc >/dev/null 2>&1; then
    export CC_riscv32imc_unknown_none_elf="$(command -v riscv32-unknown-elf-gcc)"
  elif command -v riscv64-elf-gcc >/dev/null 2>&1; then
    export CC_riscv32imc_unknown_none_elf="$(command -v riscv64-elf-gcc)"
  elif command -v brew >/dev/null 2>&1; then
    BREW_RISCV64_PREFIX="$(brew --prefix riscv64-elf-gcc 2>/dev/null || true)"
    if [[ -n "$BREW_RISCV64_PREFIX" && -x "$BREW_RISCV64_PREFIX/bin/riscv64-elf-gcc" ]]; then
      export CC_riscv32imc_unknown_none_elf="$BREW_RISCV64_PREFIX/bin/riscv64-elf-gcc"
    fi
  fi
fi

if [[ -n "${CC_riscv32imc_unknown_none_elf:-}" && -z "${CFLAGS_riscv32imc_unknown_none_elf:-}" ]]; then
  export CFLAGS_riscv32imc_unknown_none_elf="-march=rv32imc -mabi=ilp32 -ffreestanding -I${FREESTANDING_INCLUDE} -include string.h"
fi

cd "$FIRMWARE_DIR"

SQUID_FIRMWARE_BUILD_ID="$BUILD_ID" \
RUSTC="$RUSTC_PATH" \
rustup run stable cargo build --release --features hardware --bin c3-supermini-serial-hello

printf '%s\n' "$FIRMWARE_DIR/target/riscv32imc-unknown-none-elf/release/c3-supermini-serial-hello"
