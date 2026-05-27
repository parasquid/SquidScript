#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${1:-${ZEPHYR_BUILD_DIR:-${ROOT}/build/zephyr/c3-supermini}}"
LIMIT="${2:-30}"

if [[ ! -d "$BUILD_DIR" ]]; then
  printf 'Build directory not found: %s\n' "$BUILD_DIR" >&2
  exit 1
fi

mapfile -t STACK_FILES < <(find "$BUILD_DIR" -name '*.su' -type f | sort)
if [[ "${#STACK_FILES[@]}" -eq 0 ]]; then
  printf 'No .su stack-usage files found under %s\n' "$BUILD_DIR" >&2
  printf 'Build with: SQUID_ZEPHYR_STACK_USAGE=1 scripts/c3-supermini-build.sh\n' >&2
  exit 1
fi

REPORT="$(mktemp)"
SORTED_REPORT="$(mktemp)"
trap 'rm -f "$REPORT" "$SORTED_REPORT"' EXIT

printf 'bytes\tfunction\tlocation\tmode\n'
awk -F '\t' '
  NF >= 3 && $2 ~ /^[0-9]+$/ {
    location = $1
    function_name = location
    sub(/^.*:[0-9]+:[0-9]+:/, "", function_name)
    sub(/:[^:]*$/, "", location)
    printf "%s\t%s\t%s\t%s\n", $2, function_name, location, $3
  }
' "${STACK_FILES[@]}" >"$REPORT"
sort -nr -k1,1 "$REPORT" >"$SORTED_REPORT"
sed -n "1,${LIMIT}p" "$SORTED_REPORT"
