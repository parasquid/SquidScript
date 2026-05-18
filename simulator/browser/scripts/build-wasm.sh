#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUSTUP_BIN="/home/linuxbrew/.linuxbrew/opt/rustup/bin"
if [ -d "$RUSTUP_BIN" ]; then
  export PATH="$RUSTUP_BIN:$PATH"
fi
if [ -d "$HOME/.cargo/bin" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if [ -d "/var/home/linuxbrew/.linuxbrew/lib" ]; then
  export LD_LIBRARY_PATH="/var/home/linuxbrew/.linuxbrew/lib:${LD_LIBRARY_PATH:-}"
fi

WASM_PACK="${WASM_PACK:-wasm-pack}"
RUST_RUN=()

if command -v rustup >/dev/null 2>&1; then
  RUST_RUN=(rustup run stable)
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
