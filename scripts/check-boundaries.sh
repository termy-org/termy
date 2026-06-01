#!/usr/bin/env bash
set -euo pipefail

check_forbidden_dep() {
  local crate="$1"
  local forbidden_dep="$2"

  if cargo tree -p "$crate" | rg -q "\b${forbidden_dep} v"; then
    echo "Boundary check failed: ${crate} must not depend on ${forbidden_dep}" >&2
    exit 1
  fi
}

require_path() {
  local path="$1"

  if [[ ! -e "$path" ]]; then
    echo "Boundary check failed: required project path is missing: $path" >&2
    exit 1
  fi
}

forbid_pattern() {
  local pattern="$1"
  local path="$2"
  local message="$3"

  if rg -n "$pattern" "$path" >/dev/null; then
    echo "Boundary check failed: $message" >&2
    rg -n "$pattern" "$path" >&2
    exit 1
  fi
}

require_pattern() {
  local pattern="$1"
  local path="$2"
  local message="$3"

  if ! rg -n "$pattern" "$path" >/dev/null; then
    echo "Boundary check failed: $message" >&2
    exit 1
  fi
}

require_path "crates/desktop_app/Cargo.toml"
require_path "scripts/build-dmg.sh"
require_path "scripts/build-setup.ps1"
require_path "scripts/build-linux.sh"
require_path "crates/README.md"
require_path "scripts/README.md"
require_path "docs/architecture/project-layout.md"
require_path "docs/architecture/release-packaging.md"

while IFS= read -r manifest; do
  crate_dir="$(dirname "$manifest")"
  require_path "$crate_dir/README.md"
done < <(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sort)

forbid_pattern 'macos/scripts|macos/dist|TermyAlpha' \
  ".github/workflows/release.yml" \
  "release workflow must use the current scripts/ packaging paths and Termy artifact names"

require_pattern './scripts/build-dmg\.sh' \
  ".github/workflows/release.yml" \
  "release workflow must call scripts/build-dmg.sh"
require_pattern 'dist/Termy-\$\{\{ env.VERSION \}\}-macos-\$\{\{ matrix.arch \}\}\.dmg' \
  ".github/workflows/release.yml" \
  "release workflow must upload the documented macOS DMG path"

check_forbidden_dep "termy_command_core" "gpui"
check_forbidden_dep "termy_command_core" "termy_config_core"
check_forbidden_dep "termy_config_core" "termy_themes"
check_forbidden_dep "termy_cli_install_core" "gpui"
check_forbidden_dep "termy_cli" "gpui"
check_forbidden_dep "termy_core" "gpui"
check_forbidden_dep "termy_ffi" "gpui"
check_forbidden_dep "termy_ffi" "termy_terminal_ui"

cargo run -p xtask -- generate-keybindings-doc --check
cargo run -p xtask -- generate-config-doc --check

"$(dirname "${BASH_SOURCE[0]}")/check-file-sizes.sh"

echo "Boundary checks passed"
