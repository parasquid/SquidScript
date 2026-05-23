#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
elf="${1:-$repo_root/build/zephyr/c3-supermini/zephyr/zephyr.elf}"
target_json="${SQUID_ZEPHYR_TARGET_JSON:-}"
ram_profile_percent="${SQUID_ZEPHYR_RAM_PROFILE_PERCENT:-65}"
dram_limit="${SQUID_ZEPHYR_DRAM_LIMIT_BYTES:-}"
symbol_count="${SQUID_ZEPHYR_RAM_SYMBOL_COUNT:-20}"

if [[ ! -f "$elf" ]]; then
  echo "zephyr RAM audit: ELF not found: $elf" >&2
  exit 2
fi

size_tool="${SIZE:-}"
if [[ -z "$size_tool" ]]; then
  if command -v riscv32-zephyr-elf-size >/dev/null 2>&1; then
    size_tool="riscv32-zephyr-elf-size"
  else
    size_tool="size"
  fi
fi

nm_tool="${NM:-}"
if [[ -z "$nm_tool" ]]; then
  if command -v riscv32-zephyr-elf-nm >/dev/null 2>&1; then
    nm_tool="riscv32-zephyr-elf-nm"
  else
    nm_tool="nm"
  fi
fi

size_output="$("$size_tool" -A "$elf")"
dram_bytes="$(awk '$1 == "dram0_0_seg" { print $2 }' <<<"$size_output")"
if [[ -z "$dram_bytes" ]]; then
  # ESP32-C3 Zephyr size output lists DRAM sections rather than memory regions.
  dram_bytes="$(awk '
    $3 ~ /^[0-9]+$/ && $3 >= 1070071808 && $3 < 1070450448 { sum += $2 }
    END { if (sum > 0) print sum }
  ' <<<"$size_output")"
fi
if [[ -z "$dram_bytes" ]]; then
  echo "zephyr RAM audit: failed to find dram0_0_seg in $elf" >&2
  printf "%s\n" "$size_output" >&2
  exit 2
fi

target_ram_total=""
if [[ -n "$target_json" ]]; then
  if [[ ! -f "$target_json" ]]; then
    echo "zephyr RAM audit: target JSON not found: $target_json" >&2
    exit 2
  fi
  if ! [[ "$ram_profile_percent" =~ ^[0-9]+$ ]] || (( ram_profile_percent < 1 || ram_profile_percent > 100 )); then
    echo "zephyr RAM audit: SQUID_ZEPHYR_RAM_PROFILE_PERCENT must be 1..100" >&2
    exit 2
  fi
  target_ram_total="$(
    python3 - "$target_json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    target = json.load(handle)

try:
    print(int(target["mcu"]["memory"]["internalSramKB"]) * 1024)
except (KeyError, TypeError, ValueError):
    sys.exit(2)
PY
  )"
  if [[ -z "$target_ram_total" ]]; then
    echo "zephyr RAM audit: target JSON missing mcu.memory.internalSramKB" >&2
    exit 2
  fi
fi

if [[ -z "$dram_limit" ]]; then
  if [[ -n "$target_ram_total" ]]; then
    dram_limit=$(( target_ram_total * ram_profile_percent / 100 ))
  else
    dram_limit=266240
  fi
fi

if ! [[ "$dram_limit" =~ ^[0-9]+$ ]] || (( dram_limit < 1 )); then
  echo "zephyr RAM audit: SQUID_ZEPHYR_DRAM_LIMIT_BYTES must be a positive integer" >&2
  exit 2
fi

echo "dram0_0_seg=${dram_bytes} bytes limit=${dram_limit} bytes"
if [[ -n "$target_ram_total" ]]; then
  echo "target_ram_total_bytes=${target_ram_total}"
  echo "target_ram_profile_percent=${ram_profile_percent}"
  awk -v used="$dram_bytes" -v total="$target_ram_total" \
    'BEGIN { printf "target_ram_used_percent=%.1f\n", (used * 100.0) / total }'
fi
if (( dram_bytes > dram_limit )); then
  echo "zephyr RAM audit: DRAM budget exceeded" >&2
  exit 1
fi

if ! [[ "$symbol_count" =~ ^[0-9]+$ ]] || (( symbol_count < 1 )); then
  echo "zephyr RAM audit: SQUID_ZEPHYR_RAM_SYMBOL_COUNT must be a positive integer" >&2
  exit 2
fi

echo "ram_static_top_symbols=${symbol_count}"
symbol_rows="$(
  "$nm_tool" --print-size --size-sort "$elf" |
  awk '
    $1 ~ /^[0-9a-fA-F]+$/ && $2 ~ /^[0-9a-fA-F]+$/ &&
    ($3 == "b" || $3 == "B" || $3 == "d" || $3 == "D") {
      addr = strtonum("0x" $1)
      size = strtonum("0x" $2)
      if (addr >= 1070071808 && addr < 1070450448 && size < 378640) {
        printf "%u 0x%s %s %s\n", size, $1, $3, $4
      }
    }
  ' |
  tail -"$symbol_count"
)"

top_total="$(awk '{ sum += $1 } END { print sum + 0 }' <<<"$symbol_rows")"
echo "ram_static_top_bytes=${top_total}"
awk '
  NF >= 4 {
    sub(/^0x/, "", $2)
    printf "ram_symbol[%u]=size=%s addr=0x%s type=%s name=%s\n", row, $1, $2, $3, $4
    row++
  }
' <<<"$symbol_rows"
