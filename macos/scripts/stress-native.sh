#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$MACOS_DIR/.." && pwd)"
RUN_LAUNCH_SMOKE=0

usage() {
  cat <<EOF
Usage: $0 [--launch]

Run native macOS stress coverage.

Options:
  --launch   Also build and launch the app bundle with the local verify smoke.
  --help     Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --launch)
      RUN_LAUNCH_SMOKE=1
      shift
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

echo "==> Building libtermy FFI for Swift stress tests"
(cd "$REPO_ROOT" && cargo build -p termy_ffi)

echo "==> Running Swift native stress tests"
TERMY_FFI_LIBRARY_PATH="$REPO_ROOT/target/debug" \
  swift test --package-path "$MACOS_DIR" --filter TermyNativeStressTests

if [[ "$RUN_LAUNCH_SMOKE" -eq 1 ]]; then
  echo "==> Running native app launch smoke"
  "$MACOS_DIR/script/build_and_run.sh" --verify
  pkill -x TermyAlpha >/dev/null 2>&1 || true
fi
