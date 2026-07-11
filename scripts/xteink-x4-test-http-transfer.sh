#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="file-transfer-regression"
APP_DIR="${ROOT}/tests/hardware/xteink-x4/file-transfer-regression"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-transfer-http}"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
PAYLOAD_SOURCE="${PAYLOAD_SOURCE:-}"
UPLOAD_NAME="${UPLOAD_NAME:-http-transfer-smoke.binbook}"
PAYLOAD="${PAYLOAD:-${WORK_DIR}/${UPLOAD_NAME}}"
DEVICE_AP_SSID="${DEVICE_AP_SSID:-SquidScript-X4}"
DEVICE_AP_CONN="${DEVICE_AP_CONN:-squid-x4-transfer-regression}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-240}"
CURL_MAX_TIME_SECONDS="${CURL_MAX_TIME_SECONDS:-300}"
CURL_TRACE="${CURL_TRACE:-0}"
SKIP_FLASH="${SKIP_FLASH:-0}"
PORT="${PORT:-}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-http-transfer.sh [--target <id>] [--port <serial-port>] [--skip-flash] [--host-wifi-iface <iface>] [--payload <file.binbook>] [--name <file.binbook>]

Installs the X4 transfer receiver, uploads a validator-compatible generated
BinBook payload to the device AP with HTTP PUT, and verifies the copied file by
size and CRC32. Use --payload to test a specific existing BinBook.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) SKIP_FLASH=1; shift ;;
    --host-wifi-iface) HOST_WIFI_IFACE="${2:-}"; shift 2 ;;
    --payload) PAYLOAD_SOURCE="${2:-}"; shift 2 ;;
    --name) UPLOAD_NAME="${2:-}"; PAYLOAD="${WORK_DIR}/${UPLOAD_NAME}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

mkdir -p "${WORK_DIR}"

cleanup_http_transfer() {
  set +e
  if command -v nmcli >/dev/null 2>&1; then
    nmcli connection down "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-down.out" 2>&1
    nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-delete.out" 2>&1
    if [[ -n "${HOST_WIFI_IFACE}" ]]; then
      nmcli device connect "${HOST_WIFI_IFACE}" >"${WORK_DIR}/cleanup-host-wifi-connect.out" 2>&1
    fi
  fi
}
trap cleanup_http_transfer EXIT

wait_for_contains() {
  local label="$1"
  local expected="$2"
  shift 2
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${COMMAND_TIMEOUT_SECONDS}s" "$@" >"${out}" 2>&1 || true
    if grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.5
  done
  printf 'Timed out waiting for %s\n' "${expected}" >&2
  sed -n '1,160p' "${out}" >&2 || true
  capture_device_diagnostics "${label}-timeout"
  exit 1
}

detect_host_wifi_iface() {
  if [[ -n "${HOST_WIFI_IFACE}" ]]; then
    return 0
  fi
  if ! command -v nmcli >/dev/null 2>&1; then
    printf '%s\n' 'nmcli is required for the HTTP transfer hardware test' >&2
    exit 1
  fi
  HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status | awk -F: '$2 == "wifi" && $3 == "connected" { print $1; exit }')"
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status | awk -F: '$2 == "wifi" { print $1; exit }')"
  fi
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    printf '%s\n' 'No host Wi-Fi interface found for HTTP transfer hardware test' >&2
    exit 1
  fi
}

connect_host_to_device_ap() {
  detect_host_wifi_iface
  nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-delete-existing.out" 2>&1 || true
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    nmcli device wifi rescan ifname "${HOST_WIFI_IFACE}" >"${WORK_DIR}/device-ap-rescan.out" 2>&1 || true
    sleep 1
    if nmcli device wifi connect "${DEVICE_AP_SSID}" ifname "${HOST_WIFI_IFACE}" name "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-connect.out" 2>&1; then
      return 0
    fi
    sleep 1
  done
  printf '%s\n' 'Expected host Wi-Fi to associate to target AP within timeout' >&2
  exit 1
}

assert_device_ap_dhcp_lease() {
  detect_host_wifi_iface
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    nmcli -t -f IP4.ADDRESS device show "${HOST_WIFI_IFACE}" >"${WORK_DIR}/device-ap-ipv4.raw" 2>&1 || true
    if grep -Eq '192\.168\.4\.[0-9]+/24' "${WORK_DIR}/device-ap-ipv4.raw"; then
      return 0
    fi
    sleep 0.5
  done
  printf '%s\n' 'Expected host Wi-Fi to receive a target AP DHCP lease within timeout' >&2
  exit 1
}

export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi
export ESPFLASH_PORT="${PORT}"

if [[ -n "${PAYLOAD_SOURCE}" ]]; then
  cp "${PAYLOAD_SOURCE}" "${PAYLOAD}"
  write_transfer_payload_meta "${PAYLOAD}"
else
  create_transfer_binbook_payload "${PAYLOAD}"
fi
read_transfer_payload_meta "${PAYLOAD}"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
fi

run_capture package-transfer cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null
run_capture storage-format cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture install-transfer cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${PORT}" >/dev/null
run_capture launch-transfer cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${PORT}" >/dev/null
wait_for_contains output-ready "transfer ready" cargo run --quiet -p squidc -- device output --port "${PORT}" >/dev/null

connect_host_to_device_ap
assert_device_ap_dhcp_lease
curl_args=(--max-time "${CURL_MAX_TIME_SECONDS}" -fS --http1.1 --progress-bar -H "Expect:" --upload-file "${PAYLOAD}")
if [[ "${CURL_TRACE}" == "1" ]]; then
  curl_args+=(--trace-ascii "${WORK_DIR}/curl-upload.trace")
fi
curl "${curl_args[@]}" \
  -w $'\nsize_upload=%{size_upload} speed_upload=%{speed_upload} time_total=%{time_total}\n' \
  "http://192.168.4.1/upload/${UPLOAD_NAME}" >"${WORK_DIR}/curl-upload.out" \
  2> >(tee "${WORK_DIR}/curl-upload-progress.out" >&2)
grep -Fq "ok" "${WORK_DIR}/curl-upload.out"
wait_for_contains output-copy "upload copy http true null" cargo run --quiet -p squidc -- device output --port "${PORT}" >/dev/null
run_capture content-check cargo run --quiet -p squidc -- device content-check "${UPLOAD_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
if [[ -s "${errors_out}" ]]; then
  printf 'Expected device errors to be empty\n' >&2
  sed -n '1,120p' "${errors_out}" >&2
  exit 1
fi
printf 'OK XTEINK X4 HTTP transfer size=%s crc32=%s\n' "${SIZE}" "${CRC32}"
