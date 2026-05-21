#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PORT="${1:-/dev/ttyACM0}"
IMAGE="${ESP_IDF_CONTAINER_IMAGE:-docker.io/espressif/idf:release-v5.5}"

if command -v idf.py >/dev/null 2>&1; then
  idf.py -p "$PORT" flash monitor
  exit 0
fi

if command -v podman >/dev/null 2>&1; then
  podman run --rm -it \
    --group-add keep-groups \
    --device "$PORT:$PORT" \
    -v "$PWD:/project:Z" \
    -w /project \
    "$IMAGE" \
    idf.py -p "$PORT" flash monitor
  exit 0
fi

if command -v docker >/dev/null 2>&1; then
  docker run --rm -it \
    --device "$PORT:$PORT" \
    -v "$PWD:/project" \
    -w /project \
    "$IMAGE" \
    idf.py -p "$PORT" flash monitor
  exit 0
fi

cat >&2 <<'EOF'
No ESP-IDF flashing path found.

Install ESP-IDF so `idf.py` is on PATH, or install podman/docker so this script
can use the official Espressif IDF container image.
EOF
exit 1
