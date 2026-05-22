#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/c3-supermini-test-wifi-scan-api.sh is obsolete.

Wi-Fi scan coverage must be reimplemented through Zephyr Wi-Fi management and
the Zephyr command surface.
EOF
exit 1
