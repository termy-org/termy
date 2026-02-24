#!/usr/bin/env bash
# Generate .icns file from 1024px PNG for macOS app bundle
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ASSETS_DIR="$PROJECT_ROOT/assets"
ICONSET_DIR="$ASSETS_DIR/termy.iconset"
ICNS_FILE="$ASSETS_DIR/termy.icns"

if [ ! -f "$ASSETS_DIR/termy_icon@1024px.png" ]; then
    echo "Error: termy_icon@1024px.png not found in assets/"
    exit 1
fi

echo "Generating icon set from 1024px PNG..."

# Clean up any existing iconset
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"

# Generate all required icon sizes
sips -z 16 16     "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_16x16.png"
sips -z 32 32     "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_16x16@2x.png"
sips -z 32 32     "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_32x32.png"
sips -z 64 64     "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_32x32@2x.png"
sips -z 128 128   "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_128x128.png"
sips -z 256 256   "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_128x128@2x.png"
sips -z 256 256   "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_256x256.png"
sips -z 512 512   "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_256x256@2x.png"
sips -z 512 512   "$ASSETS_DIR/termy_icon@1024px.png" --out "$ICONSET_DIR/icon_512x512.png"
cp "$ASSETS_DIR/termy_icon@1024px.png" "$ICONSET_DIR/icon_512x512@2x.png"

echo "Creating .icns file..."
iconutil -c icns "$ICONSET_DIR" -o "$ICNS_FILE"

# Clean up iconset directory
rm -rf "$ICONSET_DIR"

echo "Created: $ICNS_FILE"
