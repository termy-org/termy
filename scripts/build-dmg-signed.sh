#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
Error: signed native Swift DMG packaging is not wired yet.

Use macos/scripts/build-dmg.sh for the current unsigned native DMG path.
The previous cargo-bundle GPUI signing script is archived at:
  macos/scripts/experiments/build-gpui-dmg-signed.sh
EOF

exit 2
