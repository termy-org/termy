#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$MACOS_DIR/.." && pwd)"
APP_PATH=""

usage() {
  cat <<EOF
Usage: $0 [--app PATH]

Check native Swift macOS release readiness.

Without --app this runs static checks over metadata and packaging scripts.
With --app it also validates the staged app bundle Info.plist and linkage.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      [[ $# -ge 2 ]] || { echo "Error: --app requires a value" >&2; exit 2; }
      APP_PATH="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

fail() {
  echo "Error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "'$1' is required"
}

require_cmd rg
require_cmd awk

echo "==> Checking native bundle identifiers"
if rg -n 'com\.example|PRODUCT_BUNDLE_IDENTIFIER *= *com\.example' \
  "$MACOS_DIR/Sources" "$MACOS_DIR/script" "$MACOS_DIR/scripts" >/dev/null; then
  rg -n 'com\.example|PRODUCT_BUNDLE_IDENTIFIER *= *com\.example' \
    "$MACOS_DIR/Sources" "$MACOS_DIR/script" "$MACOS_DIR/scripts" >&2
  fail "placeholder bundle identifier found in native macOS sources or scripts"
fi

source_bundle_id="$(awk -F'"' '/static let bundleIdentifier/ { print $2; exit }' "$MACOS_DIR/Sources/TermySwift/App/TermySwiftApp.swift")"
run_bundle_id="$(awk -F'"' '/^BUNDLE_ID=/ { print $2; exit }' "$MACOS_DIR/script/build_and_run.sh")"
dmg_bundle_id="$(awk -F'"' '/^BUNDLE_ID=/ { print $2; exit }' "$MACOS_DIR/scripts/build-dmg.sh")"

[[ -n "$source_bundle_id" ]] || fail "could not read AppMetadata.bundleIdentifier"
[[ "$source_bundle_id" == "$run_bundle_id" ]] || fail "build_and_run.sh bundle ID ($run_bundle_id) differs from source ($source_bundle_id)"
[[ "$source_bundle_id" == "$dmg_bundle_id" ]] || fail "build-dmg.sh bundle ID ($dmg_bundle_id) differs from source ($source_bundle_id)"
[[ "$source_bundle_id" =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\.[A-Za-z0-9][A-Za-z0-9-]*)+$ ]] || fail "bundle ID is not reverse-DNS-like: $source_bundle_id"

echo "==> Checking native signing and notarization hooks"
require_pattern() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if ! rg -n -- "$pattern" "$path" >/dev/null; then
    fail "$message"
  fi
}

require_pattern '--sign-identity' "$MACOS_DIR/scripts/build-dmg.sh" "native DMG script must accept --sign-identity"
require_pattern 'codesign' "$MACOS_DIR/scripts/build-dmg.sh" "native DMG script must sign app bundles when an identity is supplied"
require_pattern 'notarytool' "$MACOS_DIR/scripts/build-dmg.sh" "native DMG script must support notarization"
require_pattern 'stapler staple' "$MACOS_DIR/scripts/build-dmg.sh" "native DMG script must staple notarized artifacts"
require_pattern '--options runtime' "$MACOS_DIR/scripts/build-dmg.sh" "native app signing must enable hardened runtime"

if [[ -n "$APP_PATH" ]]; then
  echo "==> Checking staged native app bundle"
  [[ -d "$APP_PATH" ]] || fail "app bundle not found: $APP_PATH"
  info_plist="$APP_PATH/Contents/Info.plist"
  app_binary="$APP_PATH/Contents/MacOS/TermyAlpha"
  ffi_dylib="$APP_PATH/Contents/Frameworks/libtermy_ffi.dylib"
  [[ -f "$info_plist" ]] || fail "missing Info.plist: $info_plist"
  [[ -x "$app_binary" ]] || fail "missing executable app binary: $app_binary"
  [[ -f "$ffi_dylib" ]] || fail "missing bundled libtermy_ffi.dylib: $ffi_dylib"

  /usr/bin/plutil -lint "$info_plist" >/dev/null
  plist_bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$info_plist")"
  [[ "$plist_bundle_id" == "$source_bundle_id" ]] || fail "staged bundle ID ($plist_bundle_id) differs from source ($source_bundle_id)"

  require_cmd otool
  if ! otool -L "$app_binary" | rg -q '@rpath/libtermy_ffi\.dylib'; then
    otool -L "$app_binary" >&2
    fail "app binary must link bundled libtermy_ffi via @rpath"
  fi
fi

echo "Native macOS release readiness checks passed"
