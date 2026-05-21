#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/lib/serial-port.sh"
source "$ROOT/scripts/lib/hardware-output.sh"
PORT="$(resolve_esp_serial_port)"
cargo run -p squidc -- device reset --port "$PORT"
cargo run -p squidc -- app install --port "$PORT" tests/hardware/c3-supermini/generic-events/break-reminder.squid
cargo run -p squidc -- app install --port "$PORT" tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- app install --port "$PORT" tests/hardware/c3-supermini/generic-events/main.squid
cargo run -p squidc -- app launch --port "$PORT" main

wait_for_device_output "$PORT" "generic triggered apps" \
  'output="main start"' \
  'output="break armed"' \
  'output="reader start"' \
  'output="reader clock"' \
  'output="break fired"'

cargo run -p squidc -- device key --port "$PORT" SELECT
wait_for_device_output "$PORT" "generic triggered apps key.SELECT" \
  'output="break exit"'
printf '%s\n' "OK hardware test generic triggered apps"
