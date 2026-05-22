#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/x4-firmware-flash.sh is obsolete.

Real firmware flashing is Zephyr-only. Add an XTEINK X4 Zephyr target and
wrapper before using an X4-specific flash command.
EOF
exit 1
