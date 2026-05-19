#!/usr/bin/env bash
set -euo pipefail

PORT="${ESPFLASH_PORT:-/dev/ttyACM0}"
cargo run -p squidc -- reset --port "$PORT"
cargo run -p squidc -- install --port "$PORT" examples/timer-background/background.squid
cargo run -p squidc -- install --port "$PORT" --as main examples/timer-background/main.squid
cargo run -p squidc -- start --port "$PORT" main
sleep 3

output="$(cargo run -p squidc -- output --port "$PORT")"
printf '%s\n' "$output"

case "$output" in
  *'output="main start" 1'*'output="background register"'*'output="background timer" 1'*)
    printf '%s\n' "OK timer-background smoke"
    ;;
  *)
    printf '%s\n' "ERR timer-background smoke: expected main start, background register, and background timer output" >&2
    exit 1
    ;;
esac
