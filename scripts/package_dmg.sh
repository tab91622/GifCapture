#!/usr/bin/env bash
set -euo pipefail

APP_NAME="RecordToGif.app"
APP_BUNDLE_DIR="target/release/bundle/osx"
APP_PATH="${APP_BUNDLE_DIR}/${APP_NAME}"
DIST_DIR="dist"
DMG_NAME="RecordToGif.dmg"
DMG_PATH="${DIST_DIR}/${DMG_NAME}"

echo "[1/3] 构建 release 并生成 .app..."
cargo bundle --release

if [[ ! -d "${APP_PATH}" ]]; then
  echo "未找到 app bundle: ${APP_PATH}"
  exit 1
fi

mkdir -p "${DIST_DIR}"
rm -f "${DMG_PATH}"

echo "[2/3] 生成 .dmg..."
if command -v create-dmg >/dev/null 2>&1; then
  create-dmg \
    --volname "RecordToGif Installer" \
    --window-size 800 500 \
    --icon-size 128 \
    --app-drop-link 620 250 \
    "${DMG_PATH}" \
    "${APP_PATH}"
else
  echo "未检测到 create-dmg，回退到 hdiutil 简易打包..."
  TMP_DIR="$(mktemp -d)"
  cp -R "${APP_PATH}" "${TMP_DIR}/"
  ln -s /Applications "${TMP_DIR}/Applications"
  hdiutil create -volname "RecordToGif Installer" -srcfolder "${TMP_DIR}" -ov -format UDZO "${DMG_PATH}"
  rm -rf "${TMP_DIR}"
fi

echo "[3/3] 完成: ${DMG_PATH}"
