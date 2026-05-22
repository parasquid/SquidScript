#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SQUID_ZEPHYR_HOME="${SQUID_ZEPHYR_HOME:-${ROOT}/target/zephyr}"
VENV_DIR="${SQUID_ZEPHYR_HOME}/venv"
WORKSPACE_DIR="${SQUID_ZEPHYR_HOME}/workspace"
MANIFEST_DIR="${WORKSPACE_DIR}/SquidScript"
BREW_TOOLS=(cmake ninja dtc wget xz)

usage() {
  cat <<'USAGE'
Usage: scripts/zephyr-setup.sh [--skip-brew] [--skip-update] [--skip-blobs] [--skip-sdk]

Prepare the repo-local Zephyr tooling area.

Environment:
  SQUID_ZEPHYR_HOME  Tool/workspace root. Defaults to target/zephyr.
  PYTHON             Python interpreter for the venv. Defaults to python3.

This script may install generic host tools through Homebrew, install west into
target/zephyr/venv, initialize/update target/zephyr/workspace, and fetch the
Espressif hal_espressif blobs required by Zephyr for ESP32-C3 RF support.
USAGE
}

SKIP_BREW=0
SKIP_UPDATE=0
SKIP_BLOBS=0
SKIP_SDK=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      usage
      exit 0
      ;;
    --skip-brew)
      SKIP_BREW=1
      ;;
    --skip-update)
      SKIP_UPDATE=1
      ;;
    --skip-blobs)
      SKIP_BLOBS=1
      ;;
    --skip-sdk)
      SKIP_SDK=1
      ;;
    *)
      printf 'Unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

need_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    return 1
  fi
}

install_brew_tools() {
  local missing=()

  for tool in "${BREW_TOOLS[@]}"; do
    if ! need_command "$tool"; then
      missing+=("$tool")
    fi
  done

  if [[ "${#missing[@]}" -eq 0 ]]; then
    return
  fi

  if [[ "$SKIP_BREW" == "1" ]]; then
    printf 'Missing host tools: %s\n' "${missing[*]}" >&2
    printf 'Install them with Homebrew or re-run without --skip-brew.\n' >&2
    exit 1
  fi

  if ! need_command brew; then
    printf 'Homebrew is required to install missing host tools: %s\n' "${missing[*]}" >&2
    printf 'Install Homebrew or install the tools manually, then re-run with --skip-brew.\n' >&2
    exit 1
  fi

  brew install "${missing[@]}"
}

install_brew_tools

PYTHON="${PYTHON:-python3}"
if ! need_command "$PYTHON"; then
  printf 'Python interpreter not found: %s\n' "$PYTHON" >&2
  exit 1
fi

mkdir -p "$SQUID_ZEPHYR_HOME" "$WORKSPACE_DIR"

if [[ ! -d "$VENV_DIR" ]]; then
  "$PYTHON" -m venv "$VENV_DIR"
fi

"${VENV_DIR}/bin/python" -m pip install --upgrade pip west

source "${ROOT}/scripts/zephyr-env.sh"

if [[ -L "$MANIFEST_DIR" ]]; then
  rm "$MANIFEST_DIR"
fi

if [[ -e "$MANIFEST_DIR" && ! -d "$MANIFEST_DIR" ]]; then
  printf 'Workspace manifest path exists but is not a directory: %s\n' "$MANIFEST_DIR" >&2
  exit 1
fi

mkdir -p "$MANIFEST_DIR"
cp "${ROOT}/firmware/zephyr/west.yml" "${MANIFEST_DIR}/west.yml"

if [[ ! -d "${WORKSPACE_DIR}/.west" ]]; then
  west init -l "$MANIFEST_DIR"
fi

if [[ "$SKIP_UPDATE" != "1" ]]; then
  (cd "$WORKSPACE_DIR" && west update --path-cache "${SQUID_ZEPHYR_HOME}/west-cache")
fi

source "${ROOT}/scripts/zephyr-env.sh"

if [[ -f "${WORKSPACE_DIR}/zephyr/scripts/requirements-base.txt" ]]; then
  "${VENV_DIR}/bin/python" -m pip install -r "${WORKSPACE_DIR}/zephyr/scripts/requirements-base.txt"
fi

if [[ "$SKIP_BLOBS" != "1" ]]; then
  (cd "$WORKSPACE_DIR" && west blobs fetch hal_espressif)
fi

source "${ROOT}/scripts/zephyr-env.sh"

if [[ -z "${ZEPHYR_SDK_INSTALL_DIR:-}" && "$SKIP_SDK" != "1" ]]; then
  (cd "$WORKSPACE_DIR" && west sdk install --install-base "${SQUID_ZEPHYR_HOME}/sdk" --gnu-toolchains riscv64-zephyr-elf)
  source "${ROOT}/scripts/zephyr-env.sh"
fi

if [[ -z "${ZEPHYR_SDK_INSTALL_DIR:-}" ]]; then
  printf '%s\n' 'No Zephyr SDK directory detected.' >&2
  printf '%s\n' "Install the Zephyr SDK through Zephyr's supported SDK installer, then re-run or set ZEPHYR_SDK_INSTALL_DIR." >&2
else
  printf 'Using Zephyr SDK at %s\n' "$ZEPHYR_SDK_INSTALL_DIR"
fi

printf 'Zephyr tooling is prepared under %s\n' "$SQUID_ZEPHYR_HOME"
printf 'To use it interactively: source scripts/zephyr-env.sh\n'
