#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="${ROOT}/target/hardware-tests/display-drawlog"
DISPLAY_APP="${ROOT}/tests/hardware/c3-supermini/display-drawlog/main.squid"

mkdir -p "${WORK_DIR}"

run_capture() {
  local name="$1"
  shift
  local out="${WORK_DIR}/${name}.out"
  printf 'hardware display drawlog: %s\n' "$*" >&2
  "$@" >"${out}" 2>&1
  printf '%s\n' "${out}"
}

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

run_capture storage-format cargo run --quiet -p squidc -- device storage-format >/dev/null
run_capture install-display-drawlog cargo run --quiet -p squidc -- app install "${DISPLAY_APP}" >/dev/null

apps_out="$(run_capture app-list cargo run --quiet -p squidc -- app list)"
assert_file_contains "${apps_out}" "app=display-drawlog"

run_capture launch-display-drawlog cargo run --quiet -p squidc -- app launch display-drawlog >/dev/null

sleep 0.2
drawlog_out="$(run_capture drawlog cargo run --quiet -p squidc -- device drawlog)"
assert_file_contains "${drawlog_out}" "draw=clear color=gray0"
assert_file_contains "${drawlog_out}" "draw=select name=status"
assert_file_contains "${drawlog_out}" 'draw=image path="data/icon.bmp" x=20 y=24'
assert_file_contains "${drawlog_out}" 'draw=resource drawable="drawable/page" x=0 y=0'

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors)"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK Zephyr display drawlog SquidScript hardware check passed'
