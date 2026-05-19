#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<EOF
Usage: $0 [--version VERSION] [--arch ARCH] [--target TARGET] [--no-layout]

Options:
  --version VERSION   Set version (default: read from Cargo.toml)
  --arch ARCH         Set architecture (arm64 or x86_64)
  --target TARGET     Set target triple (aarch64-apple-darwin or x86_64-apple-darwin)
  --no-layout         Skip Finder layout customization
  --help, -h          Show this help message

Output:
  target/release/Termy-<version>-macos-<arch>.dmg
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

ensure_termy_url_scheme() {
  local app_path="$1"
  local plist_path="$app_path/Contents/Info.plist"
  local plist_buddy="/usr/libexec/PlistBuddy"

  [[ -f "$plist_path" ]] || die "App bundle Info.plist not found at $plist_path"
  [[ -x "$plist_buddy" ]] || die "PlistBuddy is required to patch $plist_path"

  "$plist_buddy" -c "Delete :CFBundleURLTypes" "$plist_path" >/dev/null 2>&1 || true
  "$plist_buddy" -c "Add :CFBundleURLTypes array" "$plist_path"
  "$plist_buddy" -c "Add :CFBundleURLTypes:0 dict" "$plist_path"
  "$plist_buddy" -c "Add :CFBundleURLTypes:0:CFBundleURLName string com.example.termy.deeplink" "$plist_path"
  "$plist_buddy" -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes array" "$plist_path"
  "$plist_buddy" -c "Add :CFBundleURLTypes:0:CFBundleURLSchemes:0 string termy" "$plist_path"
  /usr/bin/plutil -lint "$plist_path" >/dev/null
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

APP_NAME="Termy"
OS_NAME="macos"
DMG_NAME="${APP_NAME}-${VERSION}-${OS_NAME}-${ARCH}"
VOLUME_NAME="${APP_NAME}-${VERSION}"

TARGET_RELEASE_DIR="$REPO_ROOT/target/$TARGET/release"
DEFAULT_RELEASE_DIR="$REPO_ROOT/target/release"
OUTPUT_DIR="$DEFAULT_RELEASE_DIR"
DMG_ROOT="$REPO_ROOT/target/dmg-root"
RW_DMG="$OUTPUT_DIR/${DMG_NAME}-rw.dmg"
OUTPUT_DMG="$OUTPUT_DIR/${DMG_NAME}.dmg"

BUNDLE_PRIMARY="$TARGET_RELEASE_DIR/bundle/osx/$APP_NAME.app"
BUNDLE_FALLBACK="$DEFAULT_RELEASE_DIR/bundle/osx/$APP_NAME.app"

require_cmd cargo
require_cmd hdiutil

if ! cargo bundle --version >/dev/null 2>&1; then
  die "cargo-bundle not found. Install it with: cargo install cargo-bundle"
fi

if [[ ! -f "$REPO_ROOT/assets/termy.icns" || "$REPO_ROOT/assets/termy_icon@1024px.png" -nt "$REPO_ROOT/assets/termy.icns" ]]; then
  log "Generating app icon"
  "$SCRIPT_DIR/generate-icon.sh"
fi

log "Building $APP_NAME v$VERSION for $ARCH ($TARGET)"
(cd "$REPO_ROOT" && cargo build --release --target "$TARGET")
(cd "$REPO_ROOT" && cargo build --release --target "$TARGET" -p termy_cli)
(cd "$REPO_ROOT" && cargo bundle --release --format osx --target "$TARGET")

APP_PATH=""
if [[ -d "$BUNDLE_PRIMARY" ]]; then
  APP_PATH="$BUNDLE_PRIMARY"
elif [[ -d "$BUNDLE_FALLBACK" ]]; then
  APP_PATH="$BUNDLE_FALLBACK"
else
  APP_PATH="$(find "$REPO_ROOT/target" -maxdepth 5 -type d -name "$APP_NAME.app" -path "*/bundle/osx/*" | head -n1 || true)"
fi
[[ -n "$APP_PATH" && -d "$APP_PATH" ]] || die "Could not find built app bundle"

log "Registering termy:// URL scheme in app bundle"
ensure_termy_url_scheme "$APP_PATH"

# Copy CLI binary into app bundle
log "Copying CLI binary into app bundle"
CLI_BIN="$TARGET_RELEASE_DIR/termy-cli"
if [[ ! -f "$CLI_BIN" ]]; then
  CLI_BIN="$DEFAULT_RELEASE_DIR/termy-cli"
fi
if [[ -f "$CLI_BIN" ]]; then
  cp "$CLI_BIN" "$APP_PATH/Contents/MacOS/termy-cli"
  log "CLI binary copied to $APP_PATH/Contents/MacOS/termy-cli"
else
  echo "Warning: CLI binary not found, skipping CLI bundling" >&2
fi

log "Preparing DMG staging folder"
rm -rf "$DMG_ROOT"
mkdir -p "$DMG_ROOT" "$OUTPUT_DIR"
cp -R "$APP_PATH" "$DMG_ROOT/"
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
