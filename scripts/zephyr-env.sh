#!/usr/bin/env bash
# Source this file from Zephyr wrappers or an interactive shell.

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
else
  ROOT="$(pwd)"
fi

SQUID_ZEPHYR_HOME="${SQUID_ZEPHYR_HOME:-${ROOT}/target/zephyr}"
export SQUID_ZEPHYR_HOME

export ZEPHYR_BOARD="${ZEPHYR_BOARD:-esp32c3_supermini}"
export ZEPHYR_BUILD_DIR="${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}"

PATH="${SQUID_ZEPHYR_HOME}/venv/bin:${PATH}"
export PATH

ZEPHYR_BASE="${SQUID_ZEPHYR_HOME}/workspace/zephyr"
if [[ -d "$ZEPHYR_BASE" ]]; then
  export ZEPHYR_BASE
fi

if [[ -z "${ZEPHYR_SDK_INSTALL_DIR:-}" ]]; then
  for candidate in \
    "${SQUID_ZEPHYR_HOME}"/zephyr-sdk-* \
    "${SQUID_ZEPHYR_HOME}"/sdk/zephyr-sdk-* \
    "${HOME}"/zephyr-sdk-* \
    /opt/zephyr-sdk-*; do
    if [[ -d "$candidate" ]]; then
      export ZEPHYR_SDK_INSTALL_DIR="$candidate"
      break
    fi
  done
fi
