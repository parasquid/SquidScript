#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${ESPFLASH_PORT:-}"
OUT_DIR="${ROOT}/.device-backups"
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="${OUT_DIR}/xteink-x4-flash-${STAMP}.bin"
SHA="${BACKUP}.sha256"
BOOTLOADER="${OUT_DIR}/xteink-x4-bootloader-${STAMP}.bin"
PARTITION_TABLE="${OUT_DIR}/xteink-x4-partition-table-${STAMP}.bin"
RECOVERY="${OUT_DIR}/xteink-x4-recovery-${STAMP}.sh"

mkdir -p "$OUT_DIR"

if [[ -z "$PORT" ]]; then
  mapfile -t CANDIDATES < <(find /dev/serial/by-id -maxdepth 1 -type l -name 'usb-Espressif_USB_JTAG_serial_debug_unit_*-if00' 2>/dev/null | sort)
  if [[ "${#CANDIDATES[@]}" == "1" ]]; then
    PORT="${CANDIDATES[0]}"
  fi
fi

if [[ ! -e "$PORT" ]]; then
  printf 'X4 serial port not found.\n' >&2
  printf 'Set ESPFLASH_PORT=/path/to/device or connect exactly one Espressif USB JTAG serial device.\n' >&2
  exit 1
fi

if ! command -v espflash >/dev/null 2>&1; then
  printf 'espflash is required for flash backup.\n' >&2
  exit 1
fi

espflash read-flash --chip esp32c3 --port "$PORT" --non-interactive \
  --skip-update-check 0 0x1000000 "$BACKUP"
dd if="$BACKUP" of="$BOOTLOADER" bs=1 count=$((0x8000)) status=none
dd if="$BACKUP" of="$PARTITION_TABLE" bs=1 skip=$((0x8000)) count=$((0x1000)) status=none
sha256sum "$BACKUP" "$BOOTLOADER" "$PARTITION_TABLE" | tee "$SHA"

cat >"$RECOVERY" <<EOF
#!/usr/bin/env bash
set -euo pipefail
PORT="\${ESPFLASH_PORT:?set ESPFLASH_PORT to the X4 serial device}"
espflash write-bin --chip esp32c3 --port "\$PORT" --non-interactive \\
  --skip-update-check 0x0 "$BACKUP"

# Component-only recovery when the full image is not required:
# espflash write-bin --chip esp32c3 --port "\$PORT" --non-interactive \\
#   --skip-update-check 0x0 "$BOOTLOADER"
# espflash write-bin --chip esp32c3 --port "\$PORT" --non-interactive \\
#   --skip-update-check 0x8000 "$PARTITION_TABLE"
EOF
chmod 700 "$RECOVERY"

printf 'Wrote backup: %s\n' "$BACKUP"
printf 'Wrote bootloader: %s\n' "$BOOTLOADER"
printf 'Wrote partition table: %s\n' "$PARTITION_TABLE"
printf 'Wrote recovery commands: %s\n' "$RECOVERY"
