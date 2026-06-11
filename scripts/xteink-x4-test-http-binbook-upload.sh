#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

TARGET_ID="${TARGET_ID:-xteink-x4}"
APP_ID="http-binbook-upload-smoke"
APP_DIR="${ROOT}/tests/hardware/xteink-x4/http-binbook-upload"
WORK_DIR="${ROOT}/target/hardware-tests/xteink-x4-http-binbook-upload"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
BINBOOK="${BINBOOK:-${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke/books/sample.binbook}"
UPLOAD_NAME="${UPLOAD_NAME:-http-upload-smoke.binbook}"
DEVICE_AP_SSID="${DEVICE_AP_SSID:-SquidScript-X4}"
DEVICE_AP_CONN="${DEVICE_AP_CONN:-squid-x4-http-upload}"
HOST_WIFI_IFACE="${HOST_WIFI_IFACE:-}"
WAIT_TIMEOUT_SECONDS="${WAIT_TIMEOUT_SECONDS:-90}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-180}"
CURL_MAX_TIME_SECONDS="${CURL_MAX_TIME_SECONDS:-300}"
CURL_HEAD_MAX_TIME_SECONDS="${CURL_HEAD_MAX_TIME_SECONDS:-15}"
CURL_PROGRESS="${CURL_PROGRESS:-1}"
INTERRUPT_UPLOAD="${INTERRUPT_UPLOAD:-0}"
INTERRUPT_UPLOAD_SECONDS="${INTERRUPT_UPLOAD_SECONDS:-15}"
INTERRUPT_UPLOAD_LIMIT_RATE="${INTERRUPT_UPLOAD_LIMIT_RATE:-2k}"
UPLOAD_RESUME_OFFSET="${UPLOAD_RESUME_OFFSET:-}"
SKIP_FLASH="${SKIP_FLASH:-0}"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-http-binbook-upload.sh [--skip-flash] [--host-wifi-iface <iface>] [--binbook <file.binbook>]

Flashes the XTEINK X4 firmware, installs the HTTP BinBook upload smoke app,
associates the host Wi-Fi to the device AP, uploads a real .binbook with curl,
and verifies the firmware copied it into content.binbook.list("books").
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    --host-wifi-iface)
      HOST_WIFI_IFACE="${2:-}"
      if [[ -z "${HOST_WIFI_IFACE}" ]]; then
        usage >&2
        exit 2
      fi
      shift 2
      ;;
    --binbook)
      BINBOOK="${2:-}"
      if [[ -z "${BINBOOK}" ]]; then
        usage >&2
        exit 2
      fi
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

mkdir -p "${WORK_DIR}"

cleanup_http_upload() {
  set +e
  if command -v nmcli >/dev/null 2>&1; then
    nmcli connection down "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-down.out" 2>&1
    nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/cleanup-device-ap-delete.out" 2>&1
    if [[ -n "${HOST_WIFI_IFACE}" ]]; then
      nmcli device connect "${HOST_WIFI_IFACE}" >"${WORK_DIR}/cleanup-host-wifi-connect.out" 2>&1
    fi
  fi
}
trap cleanup_http_upload EXIT

assert_file_contains() {
  local file="$1"
  local expected="$2"
  if ! grep -Fq "${expected}" "${file}"; then
    printf 'Expected %s to contain: %s\n' "${file}" "${expected}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

assert_file_empty_command() {
  local file="$1"
  if [[ -s "${file}" ]]; then
    printf 'Expected %s to be empty\n' "${file}" >&2
    printf '%s\n' "--- ${file} ---" >&2
    sed -n '1,200p' "${file}" >&2
    exit 1
  fi
}

wait_for_contains() {
  local label="$1"
  local expected="$2"
  local command_name="$3"
  shift 3
  local out="${WORK_DIR}/${label}.out"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    timeout "${COMMAND_TIMEOUT_SECONDS:-20}s" "$@" >"${out}" 2>&1
    if grep -Fq "${expected}" "${out}"; then
      printf '%s\n' "${out}"
      return 0
    fi
    sleep 0.2
  done
  printf 'Timed out waiting for %s in %s\n' "${expected}" "${command_name}" >&2
  capture_device_diagnostics "${label}-timeout"
  exit 1
}

query_upload_offset() {
  local url="$1"
  local out="$2"
  local offset="0"

  if curl --max-time "${CURL_HEAD_MAX_TIME_SECONDS}" -fsSI --http1.1 "${url}" \
    >"${out}" 2>&1; then
    offset="$(awk 'BEGIN { IGNORECASE = 1 } /^X-Squid-Upload-Offset:/ { gsub("\r", "", $2); print $2; found = 1 } END { if (!found) print "0" }' "${out}")"
  fi
  if ! [[ "${offset}" =~ ^[0-9]+$ ]]; then
    printf 'Invalid upload offset from target: %s\n' "${offset}" >&2
    sed -n '1,80p' "${out}" >&2
    exit 1
  fi
  printf '%s\n' "${offset}"
}

detect_host_wifi_iface() {
  if [[ -n "${HOST_WIFI_IFACE}" ]]; then
    return 0
  fi
  if ! command -v nmcli >/dev/null 2>&1; then
    printf '%s\n' 'nmcli is required for the HTTP upload hardware test' >&2
    exit 1
  fi
  HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status |
    awk -F: '$2 == "wifi" && $3 == "connected" { print $1; exit }')"
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    HOST_WIFI_IFACE="$(nmcli -t -f DEVICE,TYPE,STATE device status |
      awk -F: '$2 == "wifi" { print $1; exit }')"
  fi
  if [[ -z "${HOST_WIFI_IFACE}" ]]; then
    printf '%s\n' 'No host Wi-Fi interface found for HTTP upload hardware test' >&2
    exit 1
  fi
}

connect_host_to_device_ap() {
  detect_host_wifi_iface
  nmcli connection delete "${DEVICE_AP_CONN}" >"${WORK_DIR}/device-ap-delete-existing.out" 2>&1 || true
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local connected=0
  while (( SECONDS < deadline )); do
    nmcli device wifi rescan ifname "${HOST_WIFI_IFACE}" >"${WORK_DIR}/device-ap-rescan.out" 2>&1 || true
    sleep 1
    if nmcli device wifi connect "${DEVICE_AP_SSID}" ifname "${HOST_WIFI_IFACE}" name "${DEVICE_AP_CONN}" \
      >"${WORK_DIR}/device-ap-connect.out" 2>&1; then
      connected=1
      break
    fi
    sleep 1
  done
  if [[ "${connected}" != "1" ]]; then
    printf '%s\n' 'Expected host Wi-Fi to associate to target AP within timeout' >&2
    exit 1
  fi
}

assert_device_ap_dhcp_lease() {
  detect_host_wifi_iface
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  while (( SECONDS < deadline )); do
    nmcli -t -f IP4.ADDRESS device show "${HOST_WIFI_IFACE}" \
      >"${WORK_DIR}/device-ap-ipv4.raw" 2>&1 || true
    if grep -Eq '192\.168\.4\.[0-9]+/24' "${WORK_DIR}/device-ap-ipv4.raw"; then
      return 0
    fi
    sleep 0.5
  done
  printf '%s\n' 'Expected host Wi-Fi to receive a target AP DHCP lease within timeout' >&2
  exit 1
}

curl_upload() {
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local url="http://192.168.4.1/upload/${UPLOAD_NAME}"
  local size
  local offset="${UPLOAD_RESUME_OFFSET}"
  size="$(stat -c '%s' "${BINBOOK}")"
  while (( SECONDS < deadline )); do
    if [[ -z "${offset}" ]]; then
      offset="$(query_upload_offset "${url}" "${WORK_DIR}/curl-upload-head.out")"
    fi
    if (( offset > size )); then
      printf 'Target upload offset %s exceeds local file size %s\n' "${offset}" "${size}" >&2
      exit 1
    fi
    printf 'upload_offset=%s upload_size=%s\n' "${offset}" "${size}" >&2
    if [[ "${CURL_PROGRESS}" == "1" ]]; then
      local curl_args=(--max-time "${CURL_MAX_TIME_SECONDS}" -fS --http1.1 --progress-bar -H "Expect:")
      if (( offset > 0 )); then
        curl_args+=(-C "${offset}")
      fi
      if curl "${curl_args[@]}" --upload-file "${BINBOOK}" \
        -w $'\nsize_upload=%{size_upload} speed_upload=%{speed_upload} time_total=%{time_total}\n' \
        "${url}" >"${WORK_DIR}/curl-upload.out" \
        2> >(tee "${WORK_DIR}/curl-upload-progress.out" >&2); then
        return 0
      fi
    else
      local curl_args=(--max-time "${CURL_MAX_TIME_SECONDS}" -fsS --http1.1 -H "Expect:")
      if (( offset > 0 )); then
        curl_args+=(-C "${offset}")
      fi
      if curl "${curl_args[@]}" --upload-file "${BINBOOK}" "${url}" \
        >"${WORK_DIR}/curl-upload.out" 2>&1; then
        return 0
      fi
    fi
    if grep -Fq "range" "${WORK_DIR}/curl-upload.out"; then
      offset=""
    fi
    sleep 1
  done
  printf '%s\n' 'Expected curl upload to target AP HTTP listener within timeout' >&2
  printf '%s\n' "--- ${WORK_DIR}/curl-upload.out ---" >&2
  sed -n '1,200p' "${WORK_DIR}/curl-upload.out" >&2
  exit 1
}

interrupt_upload_probe() {
  local url="http://192.168.4.1/upload/${UPLOAD_NAME}"
  local deadline=$((SECONDS + WAIT_TIMEOUT_SECONDS))
  local size
  size="$(stat -c '%s' "${BINBOOK}")"
  if (( size < 65536 )); then
    printf '%s\n' 'INTERRUPT_UPLOAD requires a fixture of at least 65536 bytes' >&2
    exit 1
  fi

  while (( SECONDS < deadline )); do
    set +e
    timeout "${INTERRUPT_UPLOAD_SECONDS}s" \
      curl --max-time "$((INTERRUPT_UPLOAD_SECONDS + 20))" -fS --http1.1 \
        --limit-rate "${INTERRUPT_UPLOAD_LIMIT_RATE}" --progress-bar -H "Expect:" \
        --upload-file "${BINBOOK}" "${url}" \
        >"${WORK_DIR}/curl-upload-interrupt.out" \
        2> >(tee "${WORK_DIR}/curl-upload-interrupt-progress.out" >&2)
    local status="$?"
    set -e
    printf 'interrupt_status=%s\n' "${status}" >"${WORK_DIR}/curl-upload-interrupt-status.out"
    if [[ "${status}" == "0" ]]; then
      printf '%s\n' 'Expected interrupted upload probe to stop before completion' >&2
      exit 1
    fi
    if [[ "${status}" != "7" ]]; then
      break
    fi
    sleep 1
  done

  local offset="0"
  while (( SECONDS < deadline )); do
    offset="$(query_upload_offset "${url}" "${WORK_DIR}/curl-upload-interrupt-head.out")"
    if (( offset > 0 && offset < size )); then
      printf 'interrupted_upload_offset=%s upload_size=%s\n' "${offset}" "${size}" >&2
      UPLOAD_RESUME_OFFSET="${offset}"
      return 0
    fi
    sleep 1
  done
  printf 'Expected interrupted upload to preserve a partial offset before timeout; last offset=%s size=%s\n' \
    "${offset}" "${size}" >&2
  printf '%s\n' "--- ${WORK_DIR}/curl-upload-interrupt-head.out ---" >&2
  sed -n '1,80p' "${WORK_DIR}/curl-upload-interrupt-head.out" >&2
  exit 1
}

if [[ ! -s "${BINBOOK}" ]]; then
  printf 'BinBook fixture not found or empty: %s\n' "${BINBOOK}" >&2
  exit 1
fi

source "${ROOT}/scripts/zephyr-env.sh"
export ESPFLASH_PORT="$(resolve_esp_serial_port)"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  run_capture build-x4 cargo run --quiet -p squidc -- target build --target "${TARGET_ID}" >/dev/null
  run_capture flash-x4 cargo run --quiet -p squidc -- target flash --target "${TARGET_ID}" >/dev/null
  sleep 2
fi

run_capture package-http-upload \
  cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null

run_capture reset-before-http-upload \
  cargo run --quiet -p squidc -- device reset --port "${ESPFLASH_PORT}" >/dev/null

run_capture install-http-upload \
  cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${ESPFLASH_PORT}" >/dev/null

run_capture launch-http-upload \
  cargo run --quiet -p squidc -- app launch "${APP_ID}" --port "${ESPFLASH_PORT}" >/dev/null

ready_out="$(wait_for_contains output-ready "http upload ready true null" \
  "device output" cargo run --quiet -p squidc -- device output --port "${ESPFLASH_PORT}")"
assert_file_contains "${ready_out}" "content true"

connect_host_to_device_ap
assert_device_ap_dhcp_lease

if [[ "${INTERRUPT_UPLOAD}" == "1" ]]; then
  interrupt_upload_probe
fi

curl_upload
assert_file_contains "${WORK_DIR}/curl-upload.out" "ok"

copy_out="$(wait_for_contains output-copy "upload copy true null" \
  "device output" cargo run --quiet -p squidc -- device output --port "${ESPFLASH_PORT}")"
assert_file_contains "${copy_out}" "uploaded book page ${UPLOAD_NAME}"

drawlog_out="$(run_capture drawlog cargo run --quiet -p squidc -- device drawlog --port "${ESPFLASH_PORT}")"
assert_file_contains "${drawlog_out}" "draw=binbook"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${ESPFLASH_PORT}")"
assert_file_empty_command "${errors_out}"

printf '%s\n' 'OK XTEINK X4 HTTP BinBook upload hardware check passed'
