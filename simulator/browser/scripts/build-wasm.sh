#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${RUSTUP_BIN_DIR:-}" ]; then
  export PATH="$RUSTUP_BIN_DIR:$PATH"
fi
if [ -d "$HOME/.cargo/bin" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

WASM_PACK="${WASM_PACK:-wasm-pack}"
RUST_RUN=()

if command -v rustup >/dev/null 2>&1; then
  RUST_RUN=(rustup run stable)
  RUSTC_PATH="$(rustup which --toolchain stable rustc 2>/dev/null || true)"
  CARGO_PATH="$(rustup which --toolchain stable cargo 2>/dev/null || true)"
  if [ -n "$RUSTC_PATH" ] && [ -n "$CARGO_PATH" ]; then
    export PATH="$(dirname "$RUSTC_PATH"):$PATH"
    export RUSTC="${RUSTC:-$RUSTC_PATH}"
    export CARGO="${CARGO:-$CARGO_PATH}"
  fi
fi

if ! command -v "$WASM_PACK" >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/wasm-pack" ]; then
    WASM_PACK="$HOME/.cargo/bin/wasm-pack"
  else
    echo "wasm-pack is not installed. Install it with: cargo install wasm-pack" >&2
    exit 127
  fi
fi

SYSROOT="$("${RUST_RUN[@]}" rustc --print sysroot)"
if [ ! -d "$SYSROOT/lib/rustlib/wasm32-unknown-unknown" ]; then
  echo "wasm32-unknown-unknown is not installed in the active Rust sysroot: $SYSROOT" >&2
  echo "Install a Rust toolchain with that target before building the browser compiler WASM package." >&2
  exit 1
fi

export XDG_CACHE_HOME="${XDG_CACHE_HOME:-/tmp}"

"${RUST_RUN[@]}" "$WASM_PACK" build \
  "$ROOT/../../compiler/rust/crates/squidc-wasm" \
  --target web \
  --out-dir "$ROOT/src/compiler/wasm"
