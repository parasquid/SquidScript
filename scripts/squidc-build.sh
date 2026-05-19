#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ $# -lt 1 ]]; then
  printf 'usage: %s <input.squid> --target <target-id> --out <main.sqbc>\n' "$0" >&2
  exit 2
fi

RUSTC="$(rustup which rustc)" rustup run stable cargo run \
  --manifest-path "$ROOT/Cargo.toml" \
  -p squidc \
  -- "$@"
