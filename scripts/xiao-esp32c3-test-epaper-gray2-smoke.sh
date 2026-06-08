#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ID="${TARGET_ID:-xiao-esp32c3-gdeq0426t82-sd}"
APP_ID="epaper-gray2-smoke"
APP_DIR="${ROOT}/tests/hardware/xiao-esp32c3/epaper-gray2-smoke"
BUILD_DIR="${ROOT}/build/zephyr/${TARGET_ID}"
WORK_DIR="${ROOT}/target/hardware-tests/xiao-epaper-gray2-smoke"
PACKAGE="${WORK_DIR}/${APP_ID}.squid.zip"
REFRESH_DELAY_SECONDS="${SQUID_EPAPER_GRAY2_REFRESH_DELAY_SECONDS:-8}"
SKIP_FLASH="${SKIP_FLASH:-0}"
REQUIRE_CAMERA=0
CAMERA_DEVICE="${SQUID_EPAPER_GRAY2_CAMERA:-/dev/video5}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"

source "${ROOT}/scripts/lib/hardware-command.sh"

usage() {
  cat <<'USAGE'
Usage: scripts/xiao-esp32c3-test-epaper-gray2-smoke.sh [--skip-flash] [--camera /dev/videoN] [--require-camera]

Installs and launches the retained XIAO e-paper GRAY2 hardware smoke app.
The app uses a small GRAY2 BinBook fixture to exercise product firmware display
rendering through service.display.draw. Serial output, drawlog, errors, and
resources are the unattended pass criteria. USB webcam capture is optional
evidence unless --require-camera is supplied.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-flash)
      SKIP_FLASH=1
      shift
      ;;
    --camera)
      CAMERA_DEVICE="${2:-}"
      if [[ -z "${CAMERA_DEVICE}" ]]; then
        usage >&2
        exit 2
      fi
      shift 2
      ;;
    --require-camera)
      REQUIRE_CAMERA=1
      shift
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

source "${ROOT}/scripts/zephyr-env.sh"
source "${ROOT}/scripts/lib/serial-port.sh"

mkdir -p "${WORK_DIR}"

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

capture_camera_frame() {
  local out="${WORK_DIR}/camera-gray2.jpg"

  if ! command -v ffmpeg >/dev/null 2>&1; then
    if [[ "${REQUIRE_CAMERA}" == "1" ]]; then
      printf 'ffmpeg is required for --require-camera.\n' >&2
      exit 1
    fi
    return 0
  fi
  if [[ ! -e "${CAMERA_DEVICE}" ]]; then
    if [[ "${REQUIRE_CAMERA}" == "1" ]]; then
      printf 'Camera device not found: %s\n' "${CAMERA_DEVICE}" >&2
      exit 1
    fi
    return 0
  fi

  ffmpeg -hide_banner -loglevel warning -y \
    -f v4l2 -video_size 1920x1080 -i "${CAMERA_DEVICE}" \
    -frames:v 1 -update 1 "${out}" >/dev/null 2>&1 || {
      if [[ "${REQUIRE_CAMERA}" == "1" ]]; then
        printf 'Camera capture failed: %s\n' "${CAMERA_DEVICE}" >&2
        exit 1
      fi
      return 0
    }
  printf 'Camera evidence: %s\n' "${out}"
}

export ZEPHYR_BOARD="${ZEPHYR_BOARD:-xiao_esp32c3}"
export ESPFLASH_PORT="$(resolve_esp_serial_port)"

if [[ "${SKIP_FLASH}" != "1" ]]; then
  west build -d "${BUILD_DIR}"
  west flash -d "${BUILD_DIR}"
  sleep 2
fi

run_capture package-epaper-gray2 \
  cargo run --quiet -p squidc -- app package "${APP_DIR}" --target "${TARGET_ID}" --out "${PACKAGE}" >/dev/null

run_capture install-epaper-gray2 \
  cargo run --quiet -p squidc -- app install "${PACKAGE}" --port "${ESPFLASH_PORT}" >/dev/null

run_capture launch-epaper-gray2 \
  cargo run --quiet -p squidc -- app launch ${APP_ID} --port "${ESPFLASH_PORT}" >/dev/null

launch_lifecycle_out="$(run_capture lifecycle-after-launch cargo run --quiet -p squidc -- device lifecycle --port "${ESPFLASH_PORT}")"
assert_file_contains "${launch_lifecycle_out}" "lifecycle=active=${APP_ID}"

sleep "${REFRESH_DELAY_SECONDS}"

output_out="$(run_capture output cargo run --quiet -p squidc -- device output --port "${ESPFLASH_PORT}")"
assert_file_contains "${output_out}" "gray2 pages 1"

drawlog_out="$(run_capture drawlog cargo run --quiet -p squidc -- device drawlog --port "${ESPFLASH_PORT}")"
assert_file_contains "${drawlog_out}" "draw=binbook"

errors_out="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${ESPFLASH_PORT}")"
assert_file_empty_command "${errors_out}"

run_capture resources cargo run --quiet -p squidc -- device resources --port "${ESPFLASH_PORT}" >/dev/null
capture_camera_frame

printf '%s\n' \
  'OK XIAO e-paper GRAY2 smoke serial checks passed; fixture should show black, dark gray, light gray, white bands'
