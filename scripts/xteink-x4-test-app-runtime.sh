#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT}/scripts/lib/hardware-command.sh"
TARGET_ID="${TARGET_ID:-xteink-x4}"
PORT="${PORT:-}"
WORK_DIR="${WORK_DIR:-${ROOT}/target/hardware-tests/xteink-x4-app-runtime}"
COMMAND_TIMEOUT_SECONDS="${COMMAND_TIMEOUT_SECONDS:-120}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET_ID="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --skip-flash) shift ;;
    -h|--help) printf 'Usage: %s [--target <id>] [--port <port>] [--skip-flash]\n' "$0"; exit 0 ;;
    *) exit 2 ;;
  esac
done
mkdir -p "${WORK_DIR}"
port_args=()
[[ -n "${PORT}" ]] && port_args=(--port "${PORT}")

run_capture format cargo run --quiet -p squidc -- device storage-format "${port_args[@]}" >/dev/null
for index in $(seq 0 8); do
  app_dir="${WORK_DIR}/app-${index}"
  mkdir -p "${app_dir}"
  if [[ "${index}" == 0 ]]; then
    printf '%s\n' \
      'app "runtime-0"' \
      'state { count: int = 0 }' \
      'event.on("app.start") { state.load(); debug.print("runtime", state.count) }' \
      'event.on("key.SELECT") { state.count = state.count + 1; state.save(); debug.print("saved", state.count) }' \
      >"${app_dir}/main.squid"
  else
    printf 'app "runtime-%s"\nevent.on("app.start") { debug.print("runtime", %s) }\n' \
      "${index}" "${index}" >"${app_dir}/main.squid"
  fi
  cargo run --quiet -p squidc -- app package "${app_dir}" --target "${TARGET_ID}" \
    --out "${WORK_DIR}/runtime-${index}.squid.zip" >"${WORK_DIR}/package-${index}.out" 2>&1
done
for index in $(seq 0 7); do
  run_capture "install-${index}" cargo run --quiet -p squidc -- app install \
    "${WORK_DIR}/runtime-${index}.squid.zip" "${port_args[@]}" >/dev/null
done
if cargo run --quiet -p squidc -- app install "${WORK_DIR}/runtime-8.squid.zip" \
  "${port_args[@]}" >"${WORK_DIR}/install-over-cap.out" 2>&1; then
  printf 'Expected ninth app install to fail\n' >&2
  exit 1
fi
run_capture list cargo run --quiet -p squidc -- --json app list "${port_args[@]}" >/dev/null
python3 - "${WORK_DIR}/list.out" <<'PY'
import json, sys
apps = json.load(open(sys.argv[1], encoding="utf-8"))["data"]["apps"]
assert len(apps) == 8
assert {app["appId"] for app in apps} == {f"runtime-{i}" for i in range(8)}
PY
run_capture launch cargo run --quiet -p squidc -- app launch runtime-0 "${port_args[@]}" >/dev/null
run_capture key cargo run --quiet -p squidc -- device key SELECT "${port_args[@]}" >/dev/null
run_capture reset cargo run --quiet -p squidc -- device reset "${port_args[@]}" >/dev/null
run_capture relaunch cargo run --quiet -p squidc -- app launch runtime-0 "${port_args[@]}" >/dev/null
output="$(run_capture output cargo run --quiet -p squidc -- device output "${port_args[@]}")"
grep -Fq 'output=runtime 1' "${output}"
errors="$(run_capture errors cargo run --quiet -p squidc -- device errors "${port_args[@]}")"
[[ ! -s "${errors}" ]]
printf 'OK XTEINK X4 app runtime eight-app bound and persisted state\n'
