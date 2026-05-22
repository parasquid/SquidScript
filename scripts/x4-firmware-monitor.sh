#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/x4-firmware-monitor.sh is obsolete.

Real firmware monitoring is Zephyr-only. Add an XTEINK X4 Zephyr target and
wrapper before using an X4-specific monitor command.
EOF
exit 1
