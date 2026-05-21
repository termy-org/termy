#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<EOF
Usage: $0 [--version VERSION] [--arch ARCH] [--target TARGET] [--no-layout]

Build the native Swift preview app as a drag-to-Applications DMG.

Options:
  --version VERSION   Set version (default: read from Cargo.toml)
  --arch ARCH         Set architecture (arm64 or x86_64)
  --target TARGET     Set target triple (aarch64-apple-darwin or x86_64-apple-darwin)
  --no-layout         Skip Finder layout customization
  --help, -h          Show this help message

Output:
  target/release/Termy-<version>-macos-<arch>-native-preview.dmg
EOF
}

die() {
  echo "Error: $*" >&2
  exit 1
}

log() {
  echo "==> $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' is required"
}

read_version_from_cargo_toml() {
  awk '
    /^\[package\]$/ { in_package = 1; next }
    /^\[/ && in_package { exit }
    in_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$REPO_ROOT/Cargo.toml"
}

arch_to_target() {
  case "$1" in
    arm64|aarch64) echo "aarch64-apple-darwin" ;;
    x86_64|amd64) echo "x86_64-apple-darwin" ;;
    *) return 1 ;;
  esac
}

target_to_arch() {
  case "$1" in
    aarch64-apple-darwin) echo "arm64" ;;
    x86_64-apple-darwin) echo "x86_64" ;;
    *) return 1 ;;
  esac
}

target_to_swift_triple() {
  case "$1" in
    aarch64-apple-darwin) echo "arm64-apple-macosx14.0" ;;
    x86_64-apple-darwin) echo "x86_64-apple-macosx14.0" ;;
    *) return 1 ;;
  esac
}

VERSION=""
ARCH=""
TARGET=""
SKIP_LAYOUT=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      VERSION="$2"
      shift 2
      ;;
    --arch)
      [[ $# -ge 2 ]] || die "--arch requires a value"
      ARCH="$2"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || die "--target requires a value"
      TARGET="$2"
      shift 2
      ;;
    --no-layout)
      SKIP_LAYOUT=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      die "Unknown option: $1 (use --help)"
      ;;
  esac
done

if [[ -z "$VERSION" ]]; then
  VERSION="$(read_version_from_cargo_toml)"
  [[ -n "$VERSION" ]] || die "Could not read version from Cargo.toml"
fi

if [[ -z "$ARCH" && -z "$TARGET" ]]; then
  ARCH="$(uname -m)"
fi

if [[ -n "$ARCH" && -z "$TARGET" ]]; then
  TARGET="$(arch_to_target "$ARCH")" || die "Unsupported architecture: $ARCH"
fi

if [[ -n "$TARGET" && -z "$ARCH" ]]; then
  ARCH="$(target_to_arch "$TARGET")" || die "Unsupported target: $TARGET"
fi

if [[ -n "$ARCH" && -n "$TARGET" ]]; then
  EXPECTED_TARGET="$(arch_to_target "$ARCH")" || die "Unsupported architecture: $ARCH"
  [[ "$EXPECTED_TARGET" == "$TARGET" ]] || die "Mismatched --arch ($ARCH) and --target ($TARGET)"
fi

SWIFT_TRIPLE="$(target_to_swift_triple "$TARGET")" || die "Unsupported target: $TARGET"

APP_NAME="Termy"
PRODUCT_NAME="TermySwift"
BUNDLE_ID="com.lassevestergaard.Termy.native-preview"
MIN_SYSTEM_VERSION="14.0"
ICON_SOURCE="$REPO_ROOT/assets/termy_old_icon.png"
ICON_NAME="TermyIcon"
OS_NAME="macos"
DMG_NAME="${APP_NAME}-${VERSION}-${OS_NAME}-${ARCH}-native-preview"
VOLUME_NAME="${APP_NAME}-${VERSION}-native-preview"

MACOS_DIR="$REPO_ROOT/macos"
TARGET_RELEASE_DIR="$REPO_ROOT/target/$TARGET/release"
DEFAULT_RELEASE_DIR="$REPO_ROOT/target/release"
OUTPUT_DIR="$DEFAULT_RELEASE_DIR"
DMG_ROOT="$REPO_ROOT/target/native-preview-dmg-root-$ARCH"
APP_BUNDLE="$REPO_ROOT/target/native-preview/$ARCH/$APP_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_RESOURCES="$APP_CONTENTS/Resources"
APP_FRAMEWORKS="$APP_CONTENTS/Frameworks"
APP_BINARY="$APP_MACOS/$APP_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
RW_DMG="$OUTPUT_DIR/${DMG_NAME}-rw.dmg"
OUTPUT_DMG="$OUTPUT_DIR/${DMG_NAME}.dmg"

require_cmd cargo
require_cmd swift
require_cmd hdiutil
require_cmd sips
require_cmd iconutil
require_cmd install_name_tool
require_cmd otool

build_icon() {
  local icon_tmp
  local iconset
  icon_tmp="$(mktemp -d "$REPO_ROOT/target/$ICON_NAME.XXXXXX")"
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

log "Building libtermy FFI for $ARCH ($TARGET)"
(cd "$REPO_ROOT" && cargo build --release --target "$TARGET" -p termy_ffi)

FFI_DYLIB="$TARGET_RELEASE_DIR/libtermy_ffi.dylib"
[[ -f "$FFI_DYLIB" ]] || die "Could not find built FFI library at $FFI_DYLIB"

log "Building native Swift preview for $SWIFT_TRIPLE"
(
  cd "$REPO_ROOT"
  TERMY_FFI_LIBRARY_PATH="$TARGET_RELEASE_DIR" swift build \
    --package-path "$MACOS_DIR" \
    --configuration release \
    --triple "$SWIFT_TRIPLE" \
    --product "$PRODUCT_NAME"
)

BUILD_BINARY="$(
  cd "$REPO_ROOT"
  TERMY_FFI_LIBRARY_PATH="$TARGET_RELEASE_DIR" swift build \
    --package-path "$MACOS_DIR" \
    --configuration release \
    --triple "$SWIFT_TRIPLE" \
    --show-bin-path
)/$PRODUCT_NAME"
[[ -f "$BUILD_BINARY" ]] || die "Could not find Swift build product at $BUILD_BINARY"

log "Staging $APP_NAME.app"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS" "$APP_RESOURCES" "$APP_FRAMEWORKS" "$OUTPUT_DIR"
cp "$BUILD_BINARY" "$APP_BINARY"
chmod +x "$APP_BINARY"
cp "$FFI_DYLIB" "$APP_FRAMEWORKS/libtermy_ffi.dylib"

install_name_tool -id "@rpath/libtermy_ffi.dylib" "$APP_FRAMEWORKS/libtermy_ffi.dylib"
LINKED_FFI_PATH="$(otool -L "$APP_BINARY" | awk '/libtermy_ffi\.dylib/ {print $1; exit}')"
[[ -n "$LINKED_FFI_PATH" ]] || die "$APP_BINARY is not linked against libtermy_ffi.dylib"
install_name_tool -change "$LINKED_FFI_PATH" "@rpath/libtermy_ffi.dylib" "$APP_BINARY"

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
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

/usr/bin/plutil -lint "$INFO_PLIST" >/dev/null

log "Preparing DMG staging folder"
rm -rf "$DMG_ROOT"
mkdir -p "$DMG_ROOT"
cp -R "$APP_BUNDLE" "$DMG_ROOT/"
ln -s /Applications "$DMG_ROOT/Applications"

log "Creating temporary DMG"
rm -f "$RW_DMG" "$OUTPUT_DMG"
hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$DMG_ROOT" \
  -ov \
  -fs HFS+ \
  -format UDRW \
  "$RW_DMG" >/dev/null

DEVICE=""
MOUNT_POINT=""
cleanup() {
  if [[ -n "${DEVICE:-}" ]]; then
    hdiutil detach "$DEVICE" -quiet >/dev/null 2>&1 || true
  fi
  rm -rf "$DMG_ROOT"
}
trap cleanup EXIT

ATTACH_INFO="$(hdiutil attach -readwrite -noverify -noautoopen "$RW_DMG")"
ATTACH_LINE="$(printf '%s\n' "$ATTACH_INFO" | awk '/\/Volumes\// {print; exit}')"
DEVICE="${ATTACH_LINE%%[[:space:]]*}"
MOUNT_POINT="$(printf '%s\n' "$ATTACH_LINE" | sed -E 's@.*(/Volumes/.*)$@\1@')"
[[ -n "$DEVICE" && -n "$MOUNT_POINT" && -d "$MOUNT_POINT" ]] || die "Failed to mount temporary DMG. hdiutil output: $ATTACH_INFO"

if [[ "$SKIP_LAYOUT" -eq 0 && -x "/usr/bin/osascript" ]]; then
  log "Applying Finder layout"
  if ! /usr/bin/osascript <<EOF
tell application "Finder"
  tell disk "$VOLUME_NAME"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set bounds of container window to {120, 120, 660, 440}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 128
    set text size of opts to 12
    set position of item "$APP_NAME.app" to {150, 180}
    set position of item "Applications" to {390, 180}
    update without registering applications
    delay 1
    close
  end tell
end tell
EOF
  then
    echo "Warning: Finder layout customization failed; continuing without layout tweaks" >&2
  fi
else
  log "Skipping Finder layout customization"
fi

hdiutil detach "$DEVICE" -quiet
DEVICE=""

log "Converting to compressed DMG"
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -o "$OUTPUT_DMG" >/dev/null
rm -f "$RW_DMG"

echo "Done: $OUTPUT_DMG"
