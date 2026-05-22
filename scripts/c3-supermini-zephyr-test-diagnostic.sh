#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKIP_FLASH="${SKIP_FLASH:-0}"

if [[ "$SKIP_FLASH" != "1" ]]; then
  "$ROOT/scripts/c3-supermini-zephyr-flash.sh"
else
  "$ROOT/scripts/c3-supermini-zephyr-build.sh" >/dev/null
fi

printf '%s\n' 'OK zephyr diagnostic image build/flash step completed'
printf '%s\n' 'Serial banner verification is pending Zephyr monitor automation.'
