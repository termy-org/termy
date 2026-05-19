#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<EOF
Usage: $0 [--version VERSION] [--arch ARCH] [--target TARGET] [--format FORMAT]

Options:
  --version VERSION   Set version (default: read from Cargo.toml)
  --arch ARCH         Set architecture (x86_64 or aarch64)
  --target TARGET     Set target triple (x86_64-unknown-linux-gnu or aarch64-unknown-linux-gnu)
  --format FORMAT     Output format: tarball (default) or appimage
  --help, -h          Show this help message

Output:
  target/dist/Termy-<version>-linux-<arch>.tar.gz
  or
  target/dist/Termy-<version>-linux-<arch>.AppImage
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
    x86_64|amd64) echo "x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
    *) return 1 ;;
  esac
}

target_to_arch() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "x86_64" ;;
    aarch64-unknown-linux-gnu) echo "aarch64" ;;
    *) return 1 ;;
  esac
}

VERSION=""
ARCH=""
TARGET=""
FORMAT="tarball"

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
    --format)
      [[ $# -ge 2 ]] || die "--format requires a value"
      FORMAT="$2"
      shift 2
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
APP_NAME_LOWER="termy"
OS_NAME="linux"
DIST_DIR="$REPO_ROOT/target/dist"
TARGET_RELEASE_DIR="$REPO_ROOT/target/$TARGET/release"
BINARY_PATH="$TARGET_RELEASE_DIR/$APP_NAME_LOWER"
APPIMAGETOOL_BIN="${APPIMAGETOOL:-appimagetool}"

require_cmd cargo
if [[ "$FORMAT" == "appimage" ]]; then
  if [[ "$APPIMAGETOOL_BIN" == */* ]]; then
    [[ -x "$APPIMAGETOOL_BIN" ]] || die "AppImage tool not executable: $APPIMAGETOOL_BIN"
  else
    require_cmd "$APPIMAGETOOL_BIN"
  fi
fi

log "Building $APP_NAME v$VERSION for $ARCH ($TARGET)"
(cd "$REPO_ROOT" && cargo build --release --target "$TARGET")

[[ -f "$BINARY_PATH" ]] || die "Binary not found at $BINARY_PATH"

mkdir -p "$DIST_DIR"

case "$FORMAT" in
  tarball)
    STAGING_DIR="$REPO_ROOT/target/linux-staging"
    TARBALL_NAME="${APP_NAME}-${VERSION}-${OS_NAME}-${ARCH}.tar.gz"
    OUTPUT_PATH="$DIST_DIR/$TARBALL_NAME"

    log "Creating tarball staging directory"
    rm -rf "$STAGING_DIR"
    mkdir -p "$STAGING_DIR/$APP_NAME_LOWER"

    cp "$BINARY_PATH" "$STAGING_DIR/$APP_NAME_LOWER/"

    # Copy assets if they exist
    if [[ -d "$REPO_ROOT/assets" ]]; then
      mkdir -p "$STAGING_DIR/$APP_NAME_LOWER/assets"
      cp -r "$REPO_ROOT/assets/"* "$STAGING_DIR/$APP_NAME_LOWER/assets/" 2>/dev/null || true
    fi

    # Create a simple install script
    cat > "$STAGING_DIR/$APP_NAME_LOWER/install.sh" <<'INSTALL_SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="${1:-$HOME/.local/bin}"

mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/termy" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/termy"

echo "Installed termy to $INSTALL_DIR/termy"
echo "Make sure $INSTALL_DIR is in your PATH"
INSTALL_SCRIPT
    chmod +x "$STAGING_DIR/$APP_NAME_LOWER/install.sh"

    log "Creating tarball"
    (cd "$STAGING_DIR" && tar -czvf "$OUTPUT_PATH" "$APP_NAME_LOWER")
    rm -rf "$STAGING_DIR"

    echo "Done: $OUTPUT_PATH"
    ;;

  appimage)
    APPIMAGE_STAGING_ROOT="$REPO_ROOT/target/linux-appimage-staging"
    APPDIR="$APPIMAGE_STAGING_ROOT/${APP_NAME}.AppDir"
    APPIMAGE_NAME="${APP_NAME}-${VERSION}-${OS_NAME}-${ARCH}.AppImage"
    OUTPUT_PATH="$DIST_DIR/$APPIMAGE_NAME"
    DESKTOP_FILE_SOURCE="$REPO_ROOT/packaging/linux/${APP_NAME_LOWER}.desktop"
    ICON_SOURCE="$REPO_ROOT/assets/${APP_NAME_LOWER}_icon.png"

    log "Creating AppImage staging directory"
    rm -rf "$APPIMAGE_STAGING_ROOT"
    mkdir -p \
      "$APPDIR/usr/bin" \
      "$APPDIR/usr/share/applications" \
      "$APPDIR/usr/share/icons/hicolor/512x512/apps"

    cp "$BINARY_PATH" "$APPDIR/usr/bin/$APP_NAME_LOWER"
    chmod +x "$APPDIR/usr/bin/$APP_NAME_LOWER"

    # Keep assets as a sibling to the binary, matching the tarball layout.
    if [[ -d "$REPO_ROOT/assets" ]]; then
      mkdir -p "$APPDIR/usr/bin/assets"
      cp -r "$REPO_ROOT/assets/"* "$APPDIR/usr/bin/assets/" 2>/dev/null || true
    fi

    if [[ -f "$DESKTOP_FILE_SOURCE" ]]; then
      cp "$DESKTOP_FILE_SOURCE" "$APPDIR/${APP_NAME_LOWER}.desktop"
      cp "$DESKTOP_FILE_SOURCE" "$APPDIR/usr/share/applications/${APP_NAME_LOWER}.desktop"
    else
      cat > "$APPDIR/${APP_NAME_LOWER}.desktop" <<EOF
[Desktop Entry]
Name=$APP_NAME
Exec=$APP_NAME_LOWER
Icon=$APP_NAME_LOWER
Type=Application
Categories=System;TerminalEmulator;
EOF
      cp "$APPDIR/${APP_NAME_LOWER}.desktop" "$APPDIR/usr/share/applications/${APP_NAME_LOWER}.desktop"
    fi

    [[ -f "$ICON_SOURCE" ]] || die "Linux app icon not found at $ICON_SOURCE"
    cp "$ICON_SOURCE" "$APPDIR/${APP_NAME_LOWER}.png"
    cp "$ICON_SOURCE" "$APPDIR/usr/share/icons/hicolor/512x512/apps/${APP_NAME_LOWER}.png"

    cat > "$APPDIR/AppRun" <<'APP_RUN'
#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$HERE/usr/bin/termy" "$@"
APP_RUN
    chmod +x "$APPDIR/AppRun"

    log "Creating AppImage"
    rm -f "$OUTPUT_PATH"
    if [[ "$APPIMAGETOOL_BIN" == *.AppImage ]]; then
      ARCH="$ARCH" "$APPIMAGETOOL_BIN" --appimage-extract-and-run "$APPDIR" "$OUTPUT_PATH"
    else
      ARCH="$ARCH" "$APPIMAGETOOL_BIN" "$APPDIR" "$OUTPUT_PATH"
    fi

    [[ -f "$OUTPUT_PATH" ]] || die "AppImage was not created at $OUTPUT_PATH"

    rm -rf "$APPIMAGE_STAGING_ROOT"
    echo "Done: $OUTPUT_PATH"
    ;;

  *)
    die "Unknown format: $FORMAT (use tarball or appimage)"
    ;;
esac
