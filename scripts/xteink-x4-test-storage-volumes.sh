#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
source "${ROOT}/scripts/lib/serial-port.sh"
source "${ROOT}/scripts/lib/transfer-payload.sh"

PHASE=""
CHECK_ONLY=0
PORT="${PORT:-}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-storage-volumes}"
STATE_DIR="${STATE_DIR:-${ROOT}/.device-tests/xteink-x4-storage-volumes}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-300}"
LONG_NAME="$(printf 'a%.0s' $(seq 1 113)).binbook"
DELETE_NAME="$(printf 'd%.0s' $(seq 1 113)).binbook"
INTERNAL_PAYLOAD="${STATE_DIR}/internal.binbook"
SD_PAYLOAD="${STATE_DIR}/sd.binbook"
SENTINEL_PAYLOAD="${STATE_DIR}/sd-sentinel.binbook"
SENTINEL_NAME="sd-presence.binbook"

usage() {
  cat <<'USAGE'
Usage: scripts/xteink-x4-test-storage-volumes.sh --phase prepare|absent|present [--port <port>]
       scripts/xteink-x4-test-storage-volumes.sh --check

Run `prepare` with SD inserted, physically remove the card and run `absent`,
then reinsert it and run `present`. The sentinel proves each card transition;
the phases also prove
internal fallback, cold persistence, long-name list/open/delete, SD preference,
duplicate-name precedence, and internal-format isolation.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --phase) PHASE="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --check) CHECK_ONLY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
if [[ "${CHECK_ONLY}" != 1 ]]; then
  [[ "${PHASE}" == "prepare" || "${PHASE}" == "absent" || "${PHASE}" == "present" ]] || { usage >&2; exit 2; }
fi
[[ "${#LONG_NAME}" == 121 && "${#DELETE_NAME}" == 121 ]]

mkdir -p "${WORK_DIR}" "${STATE_DIR}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

wait_for_device() {
  local deadline=$((SECONDS + 30))
  while (( SECONDS < deadline )); do
    if cargo run --quiet -p squidc -- device firmware-info --port "${PORT}" \
      >"${WORK_DIR}/device-ready.out" 2>&1; then
      return 0
    fi
    sleep 1
  done
  printf 'X4 did not return after reset\n' >&2
  return 1
}

hard_reset() {
  PATH="/var/home/tristan/codex-box/.cargo/bin:${PATH}" \
    espflash reset --chip esp32c3 --port "${PORT}" --non-interactive \
      --skip-update-check >"${WORK_DIR}/reset-${PHASE}.out" 2>&1
  wait_for_device
}

write_probe_app() {
  local app_dir="${STATE_DIR}/probe-app"
  mkdir -p "${app_dir}"
  cat >"${app_dir}/main.squid" <<EOF
app "storage-volume-probe"

event.on("app.start") {
  let listing = content.binbook.list("books", { offset: 0, limit: 8 })
  for item in listing.items max 8 {
    if item.name == "${LONG_NAME}" {
      debug.print("listed", item.name)
      let opened = binbook.open(item.ref)
      debug.print("opened", opened.ok)
    }
  }
}
EOF
}

assert_app_list_open() {
  write_probe_app
  run_capture package-probe cargo run --quiet -p squidc -- app package \
    "${STATE_DIR}/probe-app" --target xteink-x4 \
    --out "${STATE_DIR}/storage-volume-probe.squid.zip" >/dev/null
  run_capture install-probe cargo run --quiet -p squidc -- app install \
    "${STATE_DIR}/storage-volume-probe.squid.zip" --port "${PORT}" >/dev/null
  run_capture launch-probe cargo run --quiet -p squidc -- app launch \
    storage-volume-probe --port "${PORT}" >/dev/null
  local output
  output="$(run_capture probe-output cargo run --quiet -p squidc -- device output --port "${PORT}")"
  grep -Fq "output=listed ${LONG_NAME}" "${output}"
  grep -Fq 'output=opened true' "${output}"
}

if [[ "${CHECK_ONLY}" == 1 ]]; then
  write_probe_app
  cargo run --quiet -p squidc -- app package "${STATE_DIR}/probe-app" \
    --target xteink-x4 --out "${STATE_DIR}/storage-volume-probe.squid.zip" >/dev/null
  printf 'OK XTEINK X4 storage-volume runner generated 121-byte-name probe\n'
  exit 0
fi
if [[ -z "${PORT}" ]]; then
  PORT="$(resolve_esp_serial_port)"
fi

if [[ "${PHASE}" == "prepare" ]]; then
  create_transfer_binbook_payload "${SENTINEL_PAYLOAD}"
  read_transfer_payload_meta "${SENTINEL_PAYLOAD}"
  printf '%s\n' "${SIZE}" >"${STATE_DIR}/sentinel.size"
  printf '%s\n' "${CRC32}" >"${STATE_DIR}/sentinel.crc32"
  hard_reset
  run_capture put-sd-sentinel cargo run --quiet -p squidc -- device content-put \
    "${SENTINEL_PAYLOAD}" --name "${SENTINEL_NAME}" --port "${PORT}" >/dev/null
  run_capture check-sd-sentinel cargo run --quiet -p squidc -- device content-check \
    "${SENTINEL_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
  printf 'OK XTEINK X4 SD sentinel prepared; remove SD before absent phase\n'
  exit 0
fi

[[ -s "${STATE_DIR}/sentinel.size" && -s "${STATE_DIR}/sentinel.crc32" ]] || {
  printf 'Run the prepare phase with SD inserted first\n' >&2
  exit 1
}
sentinel_size="$(cat "${STATE_DIR}/sentinel.size")"
sentinel_crc="$(cat "${STATE_DIR}/sentinel.crc32")"

if [[ "${PHASE}" == "absent" ]]; then
  hard_reset
  if cargo run --quiet -p squidc -- device content-check "${SENTINEL_NAME}" \
    --size "${sentinel_size}" --crc32 "${sentinel_crc}" --port "${PORT}" \
    >"${WORK_DIR}/absent-sentinel-check.out" 2>&1; then
    printf 'SD sentinel is still readable; remove the card before absent phase\n' >&2
    exit 1
  fi
  "${ROOT}/.venv/bin/python" "${ROOT}/scripts/generate-test-binbook.py" "${INTERNAL_PAYLOAD}"
  read_transfer_payload_meta "${INTERNAL_PAYLOAD}"
  printf '%s\n' "${SIZE}" >"${STATE_DIR}/internal.size"
  printf '%s\n' "${CRC32}" >"${STATE_DIR}/internal.crc32"

  run_capture format-absent cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
  run_capture put-long-internal cargo run --quiet -p squidc -- device content-put \
    "${INTERNAL_PAYLOAD}" --name "${LONG_NAME}" --port "${PORT}" >/dev/null
  run_capture put-delete-internal cargo run --quiet -p squidc -- device content-put \
    "${INTERNAL_PAYLOAD}" --name "${DELETE_NAME}" --port "${PORT}" >/dev/null
  run_capture check-internal cargo run --quiet -p squidc -- device content-check \
    "${LONG_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
  assert_app_list_open
  hard_reset
  run_capture check-cold-internal cargo run --quiet -p squidc -- device content-check \
    "${LONG_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
  assert_app_list_open
  run_capture delete-internal cargo run --quiet -p squidc -- device content-delete \
    "${DELETE_NAME}" --port "${PORT}" >/dev/null
  if cargo run --quiet -p squidc -- device content-check "${DELETE_NAME}" \
    --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" \
    >"${WORK_DIR}/deleted-check.out" 2>&1; then
    printf 'Deleted internal content remained readable\n' >&2
    exit 1
  fi
  printf 'OK XTEINK X4 missing-SD internal fallback; insert SD before present phase\n'
  exit 0
fi

[[ -s "${INTERNAL_PAYLOAD}" && -s "${STATE_DIR}/internal.size" && -s "${STATE_DIR}/internal.crc32" ]] || {
  printf 'Run the absent phase first\n' >&2
  exit 1
}
internal_size="$(cat "${STATE_DIR}/internal.size")"
internal_crc="$(cat "${STATE_DIR}/internal.crc32")"
create_transfer_binbook_payload "${SD_PAYLOAD}"
read_transfer_payload_meta "${SD_PAYLOAD}"

hard_reset
run_capture check-returned-sentinel cargo run --quiet -p squidc -- device content-check \
  "${SENTINEL_NAME}" --size "${sentinel_size}" --crc32 "${sentinel_crc}" --port "${PORT}" >/dev/null
run_capture check-internal-with-sd cargo run --quiet -p squidc -- device content-check \
  "${LONG_NAME}" --size "${internal_size}" --crc32 "${internal_crc}" --port "${PORT}" >/dev/null
assert_app_list_open
run_capture put-sd-duplicate cargo run --quiet -p squidc -- device content-put \
  "${SD_PAYLOAD}" --name "${LONG_NAME}" --port "${PORT}" >/dev/null
run_capture check-sd-precedence cargo run --quiet -p squidc -- device content-check \
  "${LONG_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
run_capture format-internal cargo run --quiet -p squidc -- device storage-format --port "${PORT}" >/dev/null
run_capture check-sd-after-format cargo run --quiet -p squidc -- device content-check \
  "${LONG_NAME}" --size "${SIZE}" --crc32 "${CRC32}" --port "${PORT}" >/dev/null
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors --port "${PORT}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 SD preference, duplicate precedence, and format isolation\n'
