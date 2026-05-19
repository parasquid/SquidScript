#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
PORT="$(resolve_esp_serial_port)"
cargo run -p squidc -- device reset --port "$PORT"
cargo run -p squidc -- app install --port "$PORT" examples/timer-armed-app/armed.squid
cargo run -p squidc -- app install --port "$PORT" --as main examples/timer-armed-app/main.squid
cargo run -p squidc -- app launch --port "$PORT" main
sleep 3

output="$(cargo run -p squidc -- device output --port "$PORT")"
printf '%s\n' "$output"

case "$output" in
  *'output="main start" 1'*'output="armed register"'*'output="armed timer" 1'*)
    printf '%s\n' "OK hardware test timer armed app"
    ;;
  *)
    printf '%s\n' "ERR hardware test timer armed app: expected main start, armed register, and armed timer output" >&2
    exit 1
    ;;
esac
