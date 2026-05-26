#!/usr/bin/env bash

HARDWARE_COMMAND_LABEL="${HARDWARE_COMMAND_LABEL:-${0##*/}}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  local timeout_seconds="${COMMAND_TIMEOUT_SECONDS:-20}"

  printf '%s: %s\n' "${HARDWARE_COMMAND_LABEL}" "$*" >&2
  timeout "${timeout_seconds}s" "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

assert_command_fails_contains() {
  local name="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${name}.out"
  local timeout_seconds="${COMMAND_TIMEOUT_SECONDS:-20}"

  printf '%s: %s\n' "${HARDWARE_COMMAND_LABEL}" "$*" >&2
  if timeout "${timeout_seconds}s" "$@" >"${out}" 2>&1; then
    printf 'Expected command to fail: %s\n' "$*" >&2
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
    exit 1
  fi
  if ! grep -Fq "${expected}" "${out}"; then
    printf 'Expected %s to contain: %s\n' "${out}" "${expected}" >&2
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
    exit 1
  fi
}
