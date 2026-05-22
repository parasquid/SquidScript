#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/c3-supermini-test-hardware.sh [--skip-flash]

Runs the current Zephyr-backed ESP32-C3 Super Mini hardware checks
sequentially. The Zephyr command-surface suite is still being ported; this
wrapper currently runs the Zephyr diagnostic check only.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      export SKIP_FLASH=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

"$ROOT/scripts/c3-supermini-zephyr-test-diagnostic.sh"
