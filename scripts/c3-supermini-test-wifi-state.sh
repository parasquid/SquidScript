#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/c3-supermini-test-wifi-state.sh is obsolete.

Wi-Fi state coverage must be reimplemented through Zephyr Wi-Fi management and
the Zephyr command surface. Use the roadmap's Zephyr Wi-Fi tasks for the
current work items.
EOF
exit 1
