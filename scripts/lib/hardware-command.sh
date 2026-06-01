#!/usr/bin/env bash

HARDWARE_COMMAND_LABEL="${HARDWARE_COMMAND_LABEL:-${0##*/}}"

capture_device_diagnostics() {
  local label="$1"
  local timeout_seconds="${COMMAND_TIMEOUT_SECONDS:-20}"

  if [[ "${SQUID_CAPTURE_DEVICE_DIAGNOSTICS:-1}" == "0" ]]; then
    return 0
  fi
  if [[ -z "${WORK_DIR:-}" ]]; then
    return 0
  fi

  printf 'Capturing device diagnostics for %s\n' "${label}" >&2
  timeout "${timeout_seconds}s" \
    cargo run --quiet -p squidc -- device resources \
    >"${WORK_DIR}/${label}-resources.out" 2>&1 || true
  timeout "${timeout_seconds}s" \
    cargo run --quiet -p squidc -- device errors \
    >"${WORK_DIR}/${label}-errors.out" 2>&1 || true
  timeout "${timeout_seconds}s" \
    cargo run --quiet -p squidc -- device lifecycle \
    >"${WORK_DIR}/${label}-lifecycle.out" 2>&1 || true
}

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  local timeout_seconds="${COMMAND_TIMEOUT_SECONDS:-20}"

  printf '%s: %s\n' "${HARDWARE_COMMAND_LABEL}" "$*" >&2
  if timeout "${timeout_seconds}s" "$@" >"${out}" 2>&1; then
    printf '%s\n' "${out}"
  else
    local status="$?"
    printf 'Command failed or timed out after %ss: %s\n' "${timeout_seconds}" "$*" >&2
    printf '%s\n' "--- ${out} ---" >&2
    sed -n '1,200p' "${out}" >&2
    capture_device_diagnostics "${name}-failure"
    printf 'failure diagnostics: %s %s %s\n' \
      "${WORK_DIR}/${name}-failure-resources.out" \
      "${WORK_DIR}/${name}-failure-errors.out" \
      "${WORK_DIR}/${name}-failure-lifecycle.out" >&2
    return "${status}"
  fi
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
