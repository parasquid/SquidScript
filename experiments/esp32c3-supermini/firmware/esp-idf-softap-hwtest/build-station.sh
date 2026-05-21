#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

ENV_FILE="${HWTEST_ENV_FILE:-$HOME/.env}"

if [[ -f "$ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

: "${HWTEST_STA_SSID:?set HWTEST_STA_SSID to the 2.4 GHz network name for the station test}"
: "${HWTEST_STA_PASSWORD:?set HWTEST_STA_PASSWORD in $ENV_FILE before building the station test}"

export HWTEST_WIFI_MODE=station

./build.sh "$@"
