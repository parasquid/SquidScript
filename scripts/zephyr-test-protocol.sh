#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export SQUID_ZEPHYR_TARGET_JSON="${SQUID_ZEPHYR_TARGET_JSON:-${ROOT}/targets/esp32c3-super-mini.target.json}"
export SQUID_ZEPHYR_TARGET_OVERLAY="${SQUID_ZEPHYR_TARGET_OVERLAY:-${ROOT}/firmware/zephyr/boards/esp32c3_supermini.overlay}"
export SQUID_ZEPHYR_FALLBACK_SOURCE="${SQUID_ZEPHYR_FALLBACK_SOURCE:-${ROOT}/firmware/zephyr/fallback/esp32c3-supermini-main.squid}"
source "${ROOT}/scripts/zephyr-env.sh"

usage() {
  cat <<'USAGE'
Usage: scripts/zephyr-test-protocol.sh [-- <extra twister args>]

Runs the Zephyr protocol ztests through Twister on native_sim/native/64.
Defaults to `--clobber-output` so the `twister-out/` directory is deleted
between runs instead of being renamed to `twister-out.N`. Pass `-- --no-clean`
to re-use the outdir for faster incremental builds when iterating.
Extra arguments after `--` are forwarded to west twister.
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
  --clobber-output \
  "${EXTRA_ARGS[@]}"
