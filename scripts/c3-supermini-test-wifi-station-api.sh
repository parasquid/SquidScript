#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/c3-supermini-test-wifi-station-api.sh is obsolete.

Wi-Fi station coverage must be reimplemented through Zephyr Wi-Fi management
and the Zephyr command surface. Station tests should only run when credentials
are explicitly provided.
EOF
exit 1
