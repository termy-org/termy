#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$MACOS_DIR/.." && pwd)"

echo "==> Building libtermy FFI for Swift tests"
(cd "$REPO_ROOT" && cargo build -p termy_ffi)

echo "==> Running Swift config parity matrix"
TERMY_FFI_LIBRARY_PATH="$REPO_ROOT/target/debug" \
  swift test --package-path "$MACOS_DIR" --filter TermyConfigurationParityTests

echo "==> Running Swift settings schema parity matrix"
TERMY_FFI_LIBRARY_PATH="$REPO_ROOT/target/debug" \
  swift test --package-path "$MACOS_DIR" --filter SettingsSchemaParityTests
