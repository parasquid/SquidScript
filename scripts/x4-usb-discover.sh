#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${ROOT}/target/device-discovery"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG="${OUT_DIR}/xteink-x4-usb-${STAMP}.log"

mkdir -p "$OUT_DIR"

section() {
  printf "\n## %s\n\n" "$1" | tee -a "$LOG"
}

run_shell() {
  local label="$1"
  local command="$2"

  section "$label"
  printf '$ %s\n' "$command" | tee -a "$LOG"
  if bash -lc "$command" >>"$LOG" 2>&1; then
    :
  else
    printf "[command failed]\n" >>"$LOG"
  fi
}

pause() {
  local prompt="$1"
  printf "\n%s\nPress Enter to continue..." "$prompt"
  read -r _
}

capture_snapshot() {
  local name="$1"

  section "Snapshot: ${name}"
  date -Is | tee -a "$LOG"

  run_shell "USB devices" "lsusb"
  run_shell "USB tree" "lsusb -t"
  run_shell "Serial device nodes" "ls -l /dev/ttyACM* /dev/ttyUSB* 2>/dev/null || true"
  run_shell "Stable serial symlinks" "find /dev/serial -maxdepth 3 -type l -ls 2>/dev/null || true"
  run_shell "ACM udev metadata" "for dev in /dev/ttyACM*; do [ -e \"\$dev\" ] && udevadm info --query=property --name=\"\$dev\"; done; true"
  run_shell "USB serial udev metadata" "for dev in /dev/ttyUSB*; do [ -e \"\$dev\" ] && udevadm info --query=property --name=\"\$dev\"; done; true"

  if [[ "$(id -u)" == "0" ]]; then
    run_shell "Kernel log tail" "dmesg | tail -120"
  elif sudo -n true >/dev/null 2>&1; then
    run_shell "Kernel log tail" "sudo dmesg | tail -120"
  else
    section "Kernel log tail"
    {
      printf "Skipped: run this script with sudo, or configure passwordless sudo for dmesg, to capture kernel attach logs.\n"
      printf "Manual command: sudo dmesg | tail -120\n"
    } | tee -a "$LOG"
  fi
}

cat <<EOF | tee "$LOG"
# XTEINK X4 USB Discovery

This log is non-destructive. It records host USB/serial enumeration data only.

Repository: ${ROOT}
Started: $(date -Is)
Host: $(hostname)
User: $(id)

EOF

capture_snapshot "before connecting X4"

pause "Unplug the XTEINK X4, then plug it in normally."
capture_snapshot "normal USB mode"

pause "Put the XTEINK X4 into bootloader/download mode, then plug it in again."
capture_snapshot "bootloader USB mode"

section "Done"
printf "Wrote discovery log: %s\n" "$LOG" | tee -a "$LOG"
