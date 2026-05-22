#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

export HWTEST_WIFI_MODE=scan
unset HWTEST_STA_SSID
unset HWTEST_STA_PASSWORD

./build.sh "$@"
