#!/usr/bin/env bash
set -euo pipefail

# Convenience wrapper around build-dmg.sh that requires a signing identity,
# so a missing identity fails loudly instead of silently producing an
# unsigned DMG. All build-dmg.sh options are forwarded.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$*" != *"--sign-identity"* && -z "${TERMY_SIGN_IDENTITY:-}" ]]; then
  echo "Error: signed build requires --sign-identity NAME or TERMY_SIGN_IDENTITY." >&2
  echo "       For an unsigned DMG use scripts/build-dmg.sh instead." >&2
  exit 2
fi

exec "$SCRIPT_DIR/build-dmg.sh" "$@"
