#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
EXPERIMENT_DIR="${ROOT}/experiments/esp32c3-supermini/firmware/wifi-ap-probe"
FREESTANDING_INCLUDE="${ROOT}/firmware/squid-firmware/c-freestanding/include"
RUSTC_PATH="${RUSTC:-$(rustup which rustc)}"

if [[ -z "${CC_riscv32imc_unknown_none_elf:-}" ]]; then
  if command -v riscv32-unknown-elf-gcc >/dev/null 2>&1; then
    export CC_riscv32imc_unknown_none_elf="$(command -v riscv32-unknown-elf-gcc)"
  elif command -v riscv64-elf-gcc >/dev/null 2>&1; then
    export CC_riscv32imc_unknown_none_elf="$(command -v riscv64-elf-gcc)"
  fi
fi

if [[ -n "${CC_riscv32imc_unknown_none_elf:-}" && -z "${CFLAGS_riscv32imc_unknown_none_elf:-}" ]]; then
  export CFLAGS_riscv32imc_unknown_none_elf="-march=rv32imc -mabi=ilp32 -ffreestanding -I${FREESTANDING_INCLUDE} -include string.h"
fi

cd "$EXPERIMENT_DIR"
RUSTC="$RUSTC_PATH" rustup run stable cargo build --release

printf '%s\n' "$EXPERIMENT_DIR/target/riscv32imc-unknown-none-elf/release/wifi-ap-probe"
