#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/c3-supermini-test-generic-triggered-apps.sh is obsolete.

Generic triggered-app hardware coverage must be reimplemented against the
Zephyr command surface. Use scripts/c3-supermini-test-hardware.sh for current
hardware checks.
EOF
exit 1
