#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

IMAGE="${ESP_IDF_CONTAINER_IMAGE:-docker.io/espressif/idf:release-v5.5}"

if command -v idf.py >/dev/null 2>&1; then
  idf.py set-target esp32c3
  idf.py build "$@"
  exit 0
fi

if command -v podman >/dev/null 2>&1; then
  podman run --rm -t \
    -e HWTEST_WIFI_MODE \
    -e HWTEST_STA_SSID \
    -e HWTEST_STA_PASSWORD \
    -v "$PWD:/project:Z" \
    -w /project \
    "$IMAGE" \
    bash -lc 'idf.py set-target esp32c3 && idf.py build "$@"' _ "$@"
  exit 0
fi

if command -v docker >/dev/null 2>&1; then
  docker run --rm -t \
    -e HWTEST_WIFI_MODE \
    -e HWTEST_STA_SSID \
    -e HWTEST_STA_PASSWORD \
    -v "$PWD:/project" \
    -w /project \
    "$IMAGE" \
    bash -lc 'idf.py set-target esp32c3 && idf.py build "$@"' _ "$@"
  exit 0
fi

cat >&2 <<'EOF'
No ESP-IDF build path found.

Install ESP-IDF so `idf.py` is on PATH, or install podman/docker so this script
can use the official Espressif IDF container image.
EOF
exit 1
