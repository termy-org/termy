#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

cd "${SCRIPT_DIR}"

APP_NAME="libtermy-swift-example"
APP_DISPLAY_NAME="Termy"
APP_BUNDLE=".build/${APP_DISPLAY_NAME}.app"
CONTENTS_DIR="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
FRAMEWORKS_DIR="${CONTENTS_DIR}/Frameworks"
ICON_SOURCE="${REPO_ROOT}/assets/termy.icns"

if [ -t 1 ] && [ "${NO_COLOR:-}" != "1" ]; then
  C_RESET=$'\033[0m'
  C_BOLD=$'\033[1m'
  C_RED=$'\033[31m'
  C_GREEN=$'\033[32m'
  C_YELLOW=$'\033[33m'
  C_BLUE=$'\033[34m'
  C_CYAN=$'\033[36m'
else
  C_RESET=""
  C_BOLD=""
  C_RED=""
  C_GREEN=""
  C_YELLOW=""
  C_BLUE=""
  C_CYAN=""
fi

STEP_SYMBOL="->"
OK_SYMBOL="[OK]"
WARN_SYMBOL="[!]"
ERR_SYMBOL="[x]"

if [ -t 1 ]; then
  STEP_SYMBOL="=>"
  OK_SYMBOL="✓"
  WARN_SYMBOL="⚠"
  ERR_SYMBOL="✗"
fi

log_step() {
  printf "%b%s%b %s\n" "${C_CYAN}${C_BOLD}" "${STEP_SYMBOL}" "${C_RESET}" "$1"
}

log_info() {
  printf "%b%s%b %s\n" "${C_BLUE}" "i" "${C_RESET}" "$1"
}

log_ok() {
  printf "%b%s%b %s\n" "${C_GREEN}${C_BOLD}" "${OK_SYMBOL}" "${C_RESET}" "$1"
}

log_warn() {
  printf "%b%s%b %s\n" "${C_YELLOW}${C_BOLD}" "${WARN_SYMBOL}" "${C_RESET}" "$1" >&2
}

log_error() {
  printf "%b%s%b %s\n" "${C_RED}${C_BOLD}" "${ERR_SYMBOL}" "${C_RESET}" "$1" >&2
}

require_command() {
  if ! command -v "$1" > /dev/null 2>&1; then
    log_error "Required command not found: $1"
    exit 1
  fi
}

require_command cargo
require_command swift
require_command codesign
require_command install_name_tool
require_command otool
require_command open

printf "%b=== Running %s (SwiftUI dev app bundle) ===%b\n" \
  "${C_BOLD}${C_BLUE}" "${APP_DISPLAY_NAME}" "${C_RESET}"

log_step "Building libtermy FFI..."
(cd "${REPO_ROOT}" && cargo build -p termy_ffi)

log_step "Building ${APP_DISPLAY_NAME} Swift app..."
swift build

BIN_PATH="$(swift build --show-bin-path)/${APP_NAME}"

if [ ! -x "${BIN_PATH}" ]; then
  log_error "Built binary not found at ${BIN_PATH}"
  exit 1
fi

FFI_DYLIB="${REPO_ROOT}/target/debug/libtermy_ffi.dylib"
if [ ! -f "${FFI_DYLIB}" ]; then
  log_error "Built libtermy_ffi.dylib not found at ${FFI_DYLIB}"
  exit 1
fi

rm -rf "${APP_BUNDLE}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}" "${FRAMEWORKS_DIR}"

cp "${BIN_PATH}" "${MACOS_DIR}/${APP_DISPLAY_NAME}"
cp "${FFI_DYLIB}" "${FRAMEWORKS_DIR}/libtermy_ffi.dylib"

LINKED_FFI_DYLIB="$(otool -L "${MACOS_DIR}/${APP_DISPLAY_NAME}" \
  | awk '/libtermy_ffi\.dylib/ { print $1; exit }')"

if [ -n "${LINKED_FFI_DYLIB}" ] && [ "${LINKED_FFI_DYLIB}" != "@rpath/libtermy_ffi.dylib" ]; then
  log_info "Rewriting libtermy_ffi.dylib load path..."
  install_name_tool \
    -change "${LINKED_FFI_DYLIB}" "@rpath/libtermy_ffi.dylib" \
    "${MACOS_DIR}/${APP_DISPLAY_NAME}"
fi

if ! otool -l "${MACOS_DIR}/${APP_DISPLAY_NAME}" \
  | grep -A2 "LC_RPATH" \
  | grep -q "@executable_path/../Frameworks"; then
  install_name_tool -add_rpath "@executable_path/../Frameworks" "${MACOS_DIR}/${APP_DISPLAY_NAME}"
fi

if [ -f "${ICON_SOURCE}" ]; then
  cp "${ICON_SOURCE}" "${RESOURCES_DIR}/AppIcon.icns"
  log_ok "Icon embedded."
else
  log_warn "${ICON_SOURCE} not found, skipping icon."
fi

log_step "Writing Info.plist..."
cat > "${CONTENTS_DIR}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${APP_DISPLAY_NAME}</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleIdentifier</key>
  <string>com.local.termy.libtermy-swift-example</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_DISPLAY_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.0.0-dev</string>
  <key>CFBundleVersion</key>
  <string>0.0.0-dev</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
</dict>
</plist>
PLIST

log_step "Signing ${APP_DISPLAY_NAME}.app..."
codesign --force --deep --sign - "${APP_BUNDLE}" > /dev/null

log_step "Opening ${APP_BUNDLE}..."
open -n "${APP_BUNDLE}"
log_ok "App opened."
