# termy_cli

Command-line companion for Termy.

This crate owns the `termy-cli` binary, including user-facing terminal commands, config inspection helpers, theme/config utilities, and install/update commands that belong outside the desktop app.

Keep reusable install logic in `termy_cli_install_core`, release metadata logic in `termy_release_core`, and desktop UI actions in `crates/desktop_app/`.

Validation:

```sh
cargo test -p termy_cli
```
