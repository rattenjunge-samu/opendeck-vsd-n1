#!/usr/bin/env bash
set -euo pipefail

IMAGE_NAME="${IMAGE_NAME:-opendeck-n1-builder}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

echo "[1/3] Building Docker image: ${IMAGE_NAME}"
docker build -f "${SCRIPT_DIR}/Dockerfile.builder" -t "${IMAGE_NAME}" "${SCRIPT_DIR}"

echo "[2/3] Running containerized build"
docker run --rm \
  -u "$(id -u):$(id -g)" \
  -e HOME=/tmp \
  -e CARGO_HOME=/tmp/cargo \
  -v "${SCRIPT_DIR}:/work" \
  -w /work \
  "${IMAGE_NAME}" \
  bash /work/scripts/build-in-docker.sh

echo "[3/3] Build finished"
echo "Output: ${SCRIPT_DIR}/build/opendeck-n1.plugin.zip"

