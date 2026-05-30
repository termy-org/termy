# termy_command_core

Shared command catalog.

This crate owns Termy's public command identifiers, command metadata, and command/keybinding-facing definitions. It should stay pure and must not depend on GPUI or config parsing.

Use this crate when adding, renaming, documenting, or grouping user-facing commands. Wire execution in `crates/desktop_app/`.

Validation:

```sh
cargo test -p termy_command_core
just check-keybindings-doc
```
