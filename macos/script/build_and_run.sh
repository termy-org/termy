#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
APP_NAME="Termy"
PRODUCT_NAME="TermySwift"
BUNDLE_ID="com.lassevestergaard.Termy"
MIN_SYSTEM_VERSION="14.0"

MACOS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT_DIR="$(cd "$MACOS_DIR/.." && pwd)"
DIST_DIR="$MACOS_DIR/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"
APP_BINARY="$APP_MACOS/$APP_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
ICON_SOURCE="$ROOT_DIR/assets/termy_old_icon.png"
ICON_NAME="TermyIcon"

pkill -x "$APP_NAME" >/dev/null 2>&1 || true

cargo build --manifest-path "$ROOT_DIR/Cargo.toml" -p termy_ffi
swift build --package-path "$MACOS_DIR"
BUILD_BINARY="$(swift build --package-path "$MACOS_DIR" --show-bin-path)/$PRODUCT_NAME"

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS" "$APP_RESOURCES"
cp "$BUILD_BINARY" "$APP_BINARY"
chmod +x "$APP_BINARY"

build_icon() {
  local icon_tmp
  local iconset
  icon_tmp="$(mktemp -d "$DIST_DIR/$ICON_NAME.XXXXXX")"
  iconset="$icon_tmp/$ICON_NAME.iconset"
  mkdir -p "$iconset"
  trap 'rm -rf "$icon_tmp"' RETURN

  sips -z 16 16 "$ICON_SOURCE" --out "$iconset/icon_16x16.png" >/dev/null
  sips -z 32 32 "$ICON_SOURCE" --out "$iconset/icon_16x16@2x.png" >/dev/null
  sips -z 32 32 "$ICON_SOURCE" --out "$iconset/icon_32x32.png" >/dev/null
  sips -z 64 64 "$ICON_SOURCE" --out "$iconset/icon_32x32@2x.png" >/dev/null
  sips -z 128 128 "$ICON_SOURCE" --out "$iconset/icon_128x128.png" >/dev/null
  sips -z 256 256 "$ICON_SOURCE" --out "$iconset/icon_128x128@2x.png" >/dev/null
  sips -z 256 256 "$ICON_SOURCE" --out "$iconset/icon_256x256.png" >/dev/null
  sips -z 512 512 "$ICON_SOURCE" --out "$iconset/icon_256x256@2x.png" >/dev/null
  sips -z 512 512 "$ICON_SOURCE" --out "$iconset/icon_512x512.png" >/dev/null
  sips -z 1024 1024 "$ICON_SOURCE" --out "$iconset/icon_512x512@2x.png" >/dev/null
  iconutil -c icns "$iconset" -o "$APP_RESOURCES/$ICON_NAME.icns"
}

build_icon

cat >"$INFO_PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$APP_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleIconFile</key>
  <string>$ICON_NAME</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

open_app() {
  /usr/bin/open -n "$APP_BUNDLE"
}

case "$MODE" in
  run)
    open_app
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$APP_NAME\""
    ;;
  --telemetry|telemetry)
    open_app
    /usr/bin/log stream --info --style compact --predicate "subsystem == \"$BUNDLE_ID\""
    ;;
  --verify|verify)
    open_app
    sleep 1
    pgrep -x "$APP_NAME" >/dev/null
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
