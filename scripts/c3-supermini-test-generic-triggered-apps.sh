#!/usr/bin/env bash
set -euo pipefail

PORT="${ESPFLASH_PORT:-/dev/ttyACM0}"
cargo run -p squidc -- reset --port "$PORT"
cargo run -p squidc -- install --port "$PORT" tests/hardware/c3-supermini/generic-events/break-reminder.squid
cargo run -p squidc -- install --port "$PORT" tests/hardware/c3-supermini/generic-events/reader-clock.squid
cargo run -p squidc -- install --port "$PORT" tests/hardware/c3-supermini/generic-events/main.squid
cargo run -p squidc -- start --port "$PORT" main
sleep 2

output="$(cargo run -p squidc -- output --port "$PORT")"
printf '%s\n' "$output"

case "$output" in
  *'output="main start"'*'output="break armed"'*'output="reader start"'*'output="reader clock"'*'output="break fired"'*)
    ;;
  *)
    printf '%s\n' "ERR hardware test generic triggered apps: expected main, arm, reader clock, and break reminder output" >&2
    exit 1
    ;;
esac

cargo run -p squidc -- key --port "$PORT" SELECT
output_after_key="$(cargo run -p squidc -- output --port "$PORT")"
printf '%s\n' "$output_after_key"

case "$output_after_key" in
  *'output="break exit"'*)
    printf '%s\n' "OK hardware test generic triggered apps"
    ;;
  *)
    printf '%s\n' "ERR hardware test generic triggered apps: expected break exit output after SELECT" >&2
    exit 1
    ;;
esac
