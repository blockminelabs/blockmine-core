#!/bin/bash
set -euo pipefail

BINARY_PATH="${1:?Missing compiled binary path}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DIST_DIR="$ROOT/dist"
APP_NAME="Blockmine Miner"
APP_DIR="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
STAGE_DIR="$DIST_DIR/${APP_NAME}-stage"
DMG_PATH="$DIST_DIR/$APP_NAME.dmg"
ICNS_PATH="$ROOT/packaging/macos/AppIcon.icns"

if [[ "$OSTYPE" != darwin* ]]; then
  echo "Packaging must be run on macOS."
  exit 1
fi

if [ ! -f "$BINARY_PATH" ]; then
  echo "Compiled binary not found: $BINARY_PATH"
  exit 1
fi

if [ ! -f "$ICNS_PATH" ]; then
  "$SCRIPT_DIR/make-icns.sh"
fi

rm -rf "$APP_DIR" "$STAGE_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$STAGE_DIR"

cp "$BINARY_PATH" "$MACOS_DIR/Blockmine Miner"
chmod +x "$MACOS_DIR/Blockmine Miner"
cp "$ICNS_PATH" "$RESOURCES_DIR/AppIcon.icns"

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleExecutable</key>
  <string>Blockmine Miner</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>dev.blockmine.miner</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP_DIR" >/dev/null 2>&1 || true

cp -R "$APP_DIR" "$STAGE_DIR/"
ln -s /Applications "$STAGE_DIR/Applications"

cat > "$STAGE_DIR/README.txt" <<TXT
$APP_NAME

- Unified desktop miner build for macOS.
- CPU and GPU remain selectable inside the same app.
- GPU is experimental on macOS and depends on OpenCL availability.
- The app is unsigned. If macOS blocks it, right-click the app and choose Open.
TXT

rm -f "$DMG_PATH"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE_DIR" -ov -format UDZO "$DMG_PATH" >/dev/null
rm -rf "$STAGE_DIR"

echo "Packaged $APP_DIR"
echo "Packaged $DMG_PATH"
