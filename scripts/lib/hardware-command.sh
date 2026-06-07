#!/usr/bin/env bash

HARDWARE_COMMAND_LABEL="${HARDWARE_COMMAND_LABEL:-${0##*/}}"

protocol_diagnostics_empty() {
  local label="$1"

  [[ ! -s "${WORK_DIR}/${label}-resources.out" ]] &&
    [[ ! -s "${WORK_DIR}/${label}-errors.out" ]] &&
    [[ ! -s "${WORK_DIR}/${label}-lifecycle.out" ]]
}

capture_raw_serial_diagnostics() {
  local label="$1"
  local raw_seconds="${SQUID_RAW_SERIAL_DIAGNOSTIC_SECONDS:-8}"
  local out="${WORK_DIR}/${label}-raw-serial.out"
  local command=(cargo run --quiet -p squidc -- target monitor)
  local monitor_shell_command

  if [[ "${SQUID_CAPTURE_RAW_SERIAL_DIAGNOSTICS:-1}" == "0" ]]; then
    return 0
  fi
  if [[ -z "${WORK_DIR:-}" ]]; then
    return 0
  fi
  if [[ -n "${TARGET_ID:-}" ]]; then
    command+=(--target "${TARGET_ID}")
  fi
  if [[ -n "${ESPFLASH_PORT:-}" ]]; then
    command+=(--port "${ESPFLASH_PORT}")
  fi

  printf 'Capturing raw serial diagnostics for %s\n' "${label}" >&2
  monitor_shell_command="$(printf '%q ' "${command[@]}")"
  if command -v script >/dev/null 2>&1; then
    timeout "${raw_seconds}s" script -q -e -c "${monitor_shell_command}" /dev/null >"${out}" 2>&1 || true
  else
    timeout "${raw_seconds}s" "${command[@]}" >"${out}" 2>&1 || true
  fi
}

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
  if protocol_diagnostics_empty "${label}"; then
    printf 'protocol diagnostics were empty for %s; capturing raw serial\n' "${label}" >&2
    capture_raw_serial_diagnostics "${label}"
  fi
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
    if [[ -e "${WORK_DIR}/${name}-failure-raw-serial.out" ]]; then
      printf 'raw serial diagnostics: %s\n' \
        "${WORK_DIR}/${name}-failure-raw-serial.out" >&2
    fi
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
