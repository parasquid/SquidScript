#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
user_build_dir="${ZEPHYR_BUILD_DIR:-}"
user_extra_conf_file="${ZEPHYR_EXTRA_CONF_FILE:-}"
source "${ROOT}/scripts/zephyr-env.sh"

export ZEPHYR_BUILD_DIR="${user_build_dir:-${ROOT}/build/zephyr/c3-supermini-wifi-measured}"
export ZEPHYR_EXTRA_CONF_FILE="${user_extra_conf_file:-${ROOT}/firmware/zephyr/wifi-measured.conf}"

exec "${ROOT}/scripts/c3-supermini-zephyr-build.sh" "$@"
