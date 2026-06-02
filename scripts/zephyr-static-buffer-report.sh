#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
elf="${1:-$repo_root/build/zephyr/c3-supermini/zephyr/zephyr.elf}"
symbol_count="${SQUID_ZEPHYR_STATIC_SYMBOL_COUNT:-40}"

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat <<'USAGE'
Usage: scripts/zephyr-static-buffer-report.sh [path/to/zephyr.elf]

Print DRAM-resident fixed/static buffer symbols grouped by ownership.
Set SQUID_ZEPHYR_STATIC_SYMBOL_COUNT to control how many top symbols to list.
USAGE
  exit 0
fi

if [[ ! -f "$elf" ]]; then
  echo "zephyr static buffer report: ELF not found: $elf" >&2
  exit 2
fi

if ! [[ "$symbol_count" =~ ^[0-9]+$ ]] || (( symbol_count < 1 )); then
  echo "zephyr static buffer report: SQUID_ZEPHYR_STATIC_SYMBOL_COUNT must be a positive integer" >&2
  exit 2
fi

nm_tool="${NM:-}"
if [[ -z "$nm_tool" ]]; then
  if command -v riscv32-zephyr-elf-nm >/dev/null 2>&1; then
    nm_tool="riscv32-zephyr-elf-nm"
  elif command -v llvm-nm >/dev/null 2>&1; then
    nm_tool="llvm-nm"
  else
    nm_tool="nm"
  fi
fi

echo "static_buffer_elf=$elf"
echo "static_buffer_symbol_limit=$symbol_count"

symbol_rows="$(
  "$nm_tool" --print-size --size-sort "$elf" |
  awk '
    function classify(name) {
      if (name ~ /^(runtime|response|registry|install_session|temp_session|resource_session|protocol_scratch|launch_storage|trigger_storage|transport|sq_vm_runtime_work_stack)(\.|$)/) {
        return "squidscript"
      }
      if (name ~ /^(z_|kheap_|_k_|net_|mgmt_|rx_|timer_task_stack|sys_work_q_stack|logging_|service_thread|wifi_|esp|bt_|g_|g[A-Z]|s_wifi_|fdtable|server_ctx|contexts|buf32|TxRxCxt|phy_param|global_data|route_ipv4_entries|_net_buf_)/) {
        return "platform"
      }
      return "unknown"
    }
    $1 ~ /^[0-9a-fA-F]+$/ && $2 ~ /^[0-9a-fA-F]+$/ &&
    ($3 == "b" || $3 == "B" || $3 == "d" || $3 == "D") {
      addr = strtonum("0x" $1)
      size = strtonum("0x" $2)
      if (addr >= 1070071808 && addr < 1070450448 && size < 378640) {
        group = classify($4)
        group_bytes[group] += size
        group_count[group] += 1
        printf "%010u 0x%s %s %s %s\n", size, $1, $3, group, $4
      }
    }
  '
)"

awk '
  NF >= 5 {
    bytes[$4] += $1 + 0
    count[$4] += 1
  }
  END {
    for (group in bytes) {
      printf "static_buffer_group[%s]=bytes=%u count=%u\n", group, bytes[group], count[group]
    }
  }
' <<<"$symbol_rows" | sort

echo "static_buffer_top_symbols=$symbol_count"
sort -nr <<<"$symbol_rows" |
  awk '
    NR > limit {
      exit
    }
    NF >= 5 {
      printf "static_buffer_symbol[%u]=group=%s size=%u addr=%s type=%s name=%s\n", row, $4, $1 + 0, $2, $3, $5
      row++
    }
  ' limit="$symbol_count"
