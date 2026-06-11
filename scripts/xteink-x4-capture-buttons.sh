#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-${SQUID_DEVICE_PORT:-/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_38:44:BE:98:72:DC-if00}}"
DURATION_SECONDS="${DURATION_SECONDS:-120}"
INTERVAL_MS="${INTERVAL_MS:-100}"
OUT="${OUT:-${ROOT}/target/hardware-tests/xteink-x4-buttons/button-capture.log}"
COUNT="${COUNT:-$(((DURATION_SECONDS * 1000 + INTERVAL_MS - 1) / INTERVAL_MS))}"

mkdir -p "$(dirname "${OUT}")"
{
  printf '# XTEINK X4 button capture\n'
  printf '# port=%s\n' "${PORT}"
  printf '# duration_seconds=%s\n' "${DURATION_SECONDS}"
  printf '# interval_ms=%s\n' "${INTERVAL_MS}"
  printf '# count=%s\n' "${COUNT}"
  printf '# logical enum: 0=NONE 1=BACK 2=SELECT 3=LEFT 4=RIGHT 5=UP 6=DOWN\n'
  printf '# columns: epoch_ms adc1_raw adc1_logical adc1_error adc2_raw adc2_logical adc2_error power_raw power_pressed power_error\n'
} > "${OUT}"

cargo run --quiet -p squidc -- device resources --port "${PORT}" --count "${COUNT}" --interval-ms "${INTERVAL_MS}" \
  | awk -F= '
      function emit() {
        if (epoch != "" && a1 != "") {
          printf "%s %s %s %s %s %s %s %s %s %s\n", epoch, a1, l1, e1, a2, l2, e2, p, pp, pe
        }
      }
      /^sample_epoch_ms=/ {
        emit()
        epoch=$2
        a1=""; l1=""; e1=""; a2=""; l2=""; e2=""; p=""; pp=""; pe=""
      }
      /^x4\.input\.adc_gpio1_raw=/ {a1=$2}
      /^x4\.input\.adc_gpio1_logical=/ {l1=$2}
      /^x4\.input\.adc_gpio1_error=/ {e1=$2}
      /^x4\.input\.adc_gpio2_raw=/ {a2=$2}
      /^x4\.input\.adc_gpio2_logical=/ {l2=$2}
      /^x4\.input\.adc_gpio2_error=/ {e2=$2}
      /^x4\.input\.power_raw=/ {p=$2}
      /^x4\.input\.power_pressed=/ {pp=$2}
      /^x4\.input\.power_error=/ {pe=$2}
      END {
        emit()
      }' | tee -a "${OUT}"

printf 'wrote %s\n' "${OUT}"
