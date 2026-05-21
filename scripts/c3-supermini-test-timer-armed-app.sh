#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
source "$ROOT/scripts/lib/hardware-output.sh"
PORT="$(resolve_esp_serial_port)"
cargo run -p squidc -- device reset --port "$PORT"
cargo run -p squidc -- app install --port "$PORT" examples/timer-armed-app/armed.squid
cargo run -p squidc -- app install --port "$PORT" --as main examples/timer-armed-app/main.squid
cargo run -p squidc -- app launch --port "$PORT" main

wait_for_device_output "$PORT" "timer armed app" \
  'output="main start"' \
  'output="armed register"' \
  'output="armed timer"'
printf '%s\n' "OK hardware test timer armed app"
