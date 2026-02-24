set shell := ["bash", "-cu"]

# Show available recipes
@default:
    just --list

run:
    cargo run --release

build:
    cargo build --release

# Generate macOS .icns file from assets/termy_icon@1024px.png
generate-icon:
    ./scripts/generate-icon.sh

# Build macOS app bundle and DMG
# Example:

# just build-dmg -- --version 0.1.0 --arch arm64 --target aarch64-apple-darwin
build-dmg *args:
    set -- {{ args }}; \
    if [ "${1-}" = "--" ]; then shift; fi; \
    ./scripts/build-dmg.sh "$@"

# Build signed/notarized macOS DMG
# Example:

# just build-dmg-signed -- --sign-identity "Developer ID Application: Your Name (TEAMID)" --notary-profile TERMY_NOTARY
build-dmg-signed *args:
    set -- {{ args }}; \
    if [ "${1-}" = "--" ]; then shift; fi; \
    ./scripts/build-dmg-signed.sh "$@"

# Build Windows Setup.exe via Inno Setup
# Example:

# just build-setup -- -Version 0.1.0 -Arch x64 -Target x86_64-pc-windows-msvc
build-setup *args:
    set -- {{ args }}; \
    if [ "${1-}" = "--" ]; then shift; fi; \
    if command -v powershell >/dev/null 2>&1; then \
      powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/build-setup.ps1 "$@"; \
    elif command -v powershell.exe >/dev/null 2>&1; then \
      powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./scripts/build-setup.ps1 "$@"; \
    elif [ -x /c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe ]; then \
      /c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -ExecutionPolicy Bypass -File ./scripts/build-setup.ps1 "$@"; \
    else \
      echo "PowerShell not found. Install PowerShell or run scripts/build-setup.ps1 directly from PowerShell."; \
      exit 127; \
    fi
