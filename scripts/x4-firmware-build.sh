#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
scripts/x4-firmware-build.sh is obsolete.

Real firmware builds are Zephyr-only. Use the current Zephyr wrappers or add an
XTEINK X4 Zephyr target before restoring an X4-specific build command.
EOF
exit 1
