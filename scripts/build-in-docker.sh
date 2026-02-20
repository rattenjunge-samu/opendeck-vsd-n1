#!/usr/bin/env bash
set -euo pipefail

PLUGIN_ID="com.github.ambiso.opendeck-akp05.sdPlugin"
TARGET_TRIPLE="x86_64-unknown-linux-gnu"
TARGET_DIR="target/plugin-linux"
BIN_PATH="${TARGET_DIR}/${TARGET_TRIPLE}/release/opendeck-n1"

echo "[1/4] Building Linux binary (${TARGET_TRIPLE})"
cargo build --release --target "${TARGET_TRIPLE}" --target-dir "${TARGET_DIR}"

echo "[2/4] Preparing plugin directory"
rm -rf build
mkdir -p "build/${PLUGIN_ID}"
cp -r assets "build/${PLUGIN_ID}"
cp manifest.json "build/${PLUGIN_ID}"
cp "${BIN_PATH}" "build/${PLUGIN_ID}/opendeck-n1-linux"

echo "[3/4] Creating ZIP package"
(
  cd build
  zip -r opendeck-n1.plugin.zip "${PLUGIN_ID}"
)

echo "[4/4] Done"
echo "Package created at: build/opendeck-n1.plugin.zip"

