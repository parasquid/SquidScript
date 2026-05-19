#!/usr/bin/env bash

resolve_esp_serial_port() {
  if [[ -n "${ESPFLASH_PORT:-}" ]]; then
    printf '%s\n' "$ESPFLASH_PORT"
    return 0
  fi

  local candidates=()
  if [[ -d /dev/serial/by-id ]]; then
    while IFS= read -r path; do
      candidates+=("$path")
    done < <(find /dev/serial/by-id -maxdepth 1 -type l \( \
      -name 'usb-Espressif_USB_JTAG_serial_debug_unit_*-if00' -o \
      -name '*Espressif*' \
    \) 2>/dev/null | sort)
  fi

  local pattern
  for pattern in /dev/cu.usbmodem* /dev/cu.SLAB_USBtoUART* /dev/ttyACM* /dev/ttyUSB*; do
    for path in $pattern; do
      [[ -e "$path" ]] || continue
      candidates+=("$path")
    done
  done

  local unique=()
  local unique_real=()
  local seen
  local real
  for path in "${candidates[@]}"; do
    real="$(realpath "$path" 2>/dev/null || printf '%s\n' "$path")"
    seen=0
    for existing in "${unique_real[@]}"; do
      if [[ "$existing" == "$real" ]]; then
        seen=1
        break
      fi
    done
    if [[ "$seen" == "0" ]]; then
      unique+=("$path")
      unique_real+=("$real")
    fi
  done

  if [[ "${#unique[@]}" == "1" ]]; then
    printf '%s\n' "${unique[0]}"
    return 0
  fi

  if [[ "${#unique[@]}" == "0" ]]; then
    printf 'No Espressif serial port found. Set ESPFLASH_PORT=/path/to/device.\n' >&2
  else
    printf 'Multiple serial candidates found. Set ESPFLASH_PORT to one of:\n' >&2
    printf '  %s\n' "${unique[@]}" >&2
  fi
  return 1
}
