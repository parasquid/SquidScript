#!/usr/bin/env bash

output_contains_in_order() {
  local remaining="$1"
  shift

  local needle
  for needle in "$@"; do
    if [[ "$remaining" != *"$needle"* ]]; then
      return 1
    fi
    remaining="${remaining#*"$needle"}"
  done
}

wait_for_device_output() {
  local port="$1"
  local label="$2"
  shift 2

  local interval="${HARDWARE_TEST_OUTPUT_POLL_INTERVAL:-0.2}"
  local timeout_seconds="${COMMAND_TIMEOUT_SECONDS:-20}"
  local wait_seconds="${HARDWARE_TEST_OUTPUT_WAIT_SECONDS:-20}"
  local deadline=$((SECONDS + wait_seconds))
  local output=""

  while (( SECONDS < deadline )); do
    if ! output="$(timeout "${timeout_seconds}s" cargo run -p squidc -- device output --port "$port" 2>&1)"; then
      printf '%s\n' "$output"
      printf 'ERR hardware test %s: device output command failed or timed out\n' "$label" >&2
      return 1
    fi
    if output_contains_in_order "$output" "$@"; then
      printf '%s\n' "$output"
      return 0
    fi
    sleep "$interval"
  done

  printf '%s\n' "$output"
  printf 'ERR hardware test %s: timed out waiting for expected output sequence\n' "$label" >&2
  return 1
}
