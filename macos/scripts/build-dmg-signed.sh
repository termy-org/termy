#!/usr/bin/env bash
set -euo pipefail

# Convenience wrapper around the native Swift DMG builder that requires a
# signing identity. Use macos/scripts/build-dmg.sh directly for unsigned builds.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$*" != *"--sign-identity"* && -z "${TERMY_SIGN_IDENTITY:-}" ]]; then
  echo "Error: signed native build requires --sign-identity NAME or TERMY_SIGN_IDENTITY." >&2
  echo "       For an unsigned DMG use macos/scripts/build-dmg.sh instead." >&2
  exit 2
fi

exec "$SCRIPT_DIR/build-dmg.sh" "$@"
