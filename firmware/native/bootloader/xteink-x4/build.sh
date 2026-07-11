#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
image="${ESP_IDF_CONTAINER_IMAGE:-docker.io/espressif/idf:release-v5.5}"

if command -v idf.py >/dev/null 2>&1; then
  idf.py set-target esp32c3
  idf.py bootloader
elif command -v podman >/dev/null 2>&1; then
  podman run --rm -t -v "$PWD:/project:Z" -w /project "$image" \
    bash -lc 'idf.py set-target esp32c3 && idf.py bootloader'
elif command -v distrobox-host-exec >/dev/null 2>&1; then
  distrobox-host-exec podman run --rm -t -v "$PWD:/project:Z" -w /project "$image" \
    bash -lc 'idf.py set-target esp32c3 && idf.py bootloader'
else
  echo "ESP-IDF or Podman is required to build the X4 bootloader" >&2
  exit 1
fi

cp build/bootloader/bootloader.bin bootloader.bin
