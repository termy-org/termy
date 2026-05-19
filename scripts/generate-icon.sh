#!/usr/bin/env bash
# Generate app icon files from the canonical 1024px PNG.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ASSETS_DIR="$PROJECT_ROOT/assets"
ICONSET_DIR="$ASSETS_DIR/termy.iconset"
ICNS_FILE="$ASSETS_DIR/termy.icns"
PNG_FILE="$ASSETS_DIR/termy_icon.png"
ICO_FILE="$ASSETS_DIR/termy.ico"
SOURCE_PNG="$ASSETS_DIR/termy_icon@1024px.png"

if [ ! -f "$SOURCE_PNG" ]; then
    echo "Error: termy_icon@1024px.png not found in assets/"
    exit 1
fi

echo "Generating 512px PNG..."
sips -z 512 512 "$SOURCE_PNG" --out "$PNG_FILE" >/dev/null

echo "Generating icon set from 1024px PNG..."

# Clean up any existing iconset
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"

# Generate all required icon sizes
sips -z 16 16     "$SOURCE_PNG" --out "$ICONSET_DIR/icon_16x16.png"
sips -z 32 32     "$SOURCE_PNG" --out "$ICONSET_DIR/icon_16x16@2x.png"
sips -z 32 32     "$SOURCE_PNG" --out "$ICONSET_DIR/icon_32x32.png"
sips -z 64 64     "$SOURCE_PNG" --out "$ICONSET_DIR/icon_32x32@2x.png"
sips -z 128 128   "$SOURCE_PNG" --out "$ICONSET_DIR/icon_128x128.png"
sips -z 256 256   "$SOURCE_PNG" --out "$ICONSET_DIR/icon_128x128@2x.png"
sips -z 256 256   "$SOURCE_PNG" --out "$ICONSET_DIR/icon_256x256.png"
sips -z 512 512   "$SOURCE_PNG" --out "$ICONSET_DIR/icon_256x256@2x.png"
sips -z 512 512   "$SOURCE_PNG" --out "$ICONSET_DIR/icon_512x512.png"
cp "$SOURCE_PNG" "$ICONSET_DIR/icon_512x512@2x.png"

echo "Creating .icns file..."
iconutil -c icns "$ICONSET_DIR" -o "$ICNS_FILE"

# Clean up iconset directory
rm -rf "$ICONSET_DIR"

echo "Created: $ICNS_FILE"

if command -v magick >/dev/null 2>&1; then
    echo "Creating .ico file..."
    magick "$SOURCE_PNG" -define icon:auto-resize=256,128,64,48,32,16 "$ICO_FILE"
    echo "Created: $ICO_FILE"
else
    echo "Warning: ImageMagick 'magick' not found; skipping .ico generation" >&2
fi
