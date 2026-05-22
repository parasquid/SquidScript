#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/c3-supermini-test-reference-firmware.sh is obsolete.

Real firmware tests are Zephyr-only. Use:

  scripts/c3-supermini-test-hardware.sh

or the current diagnostic slice:

  scripts/c3-supermini-zephyr-test-diagnostic.sh
EOF
exit 1
