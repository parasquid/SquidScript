#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
  cat <<'USAGE'
Usage: scripts/zephyr-test-protocol.sh [-- <extra twister args>]

Runs the Zephyr protocol ztests through Twister on native_sim/native/64.
Extra arguments after -- are forwarded to west twister.
USAGE
}

EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --)
      shift
      EXTRA_ARGS=("$@")
      break
      ;;
    *)
      EXTRA_ARGS+=("$1")
      ;;
  esac
  shift
done

west twister \
  -T "${ROOT}/firmware/zephyr/tests/protocol" \
  --platform native_sim/native/64 \
  --inline-logs \
  "${EXTRA_ARGS[@]}"
