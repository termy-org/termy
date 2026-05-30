# xtask

Repository automation binary.

This crate owns maintainer commands that generate or verify repository artifacts, such as generated keybinding and configuration documentation.

Keep product runtime code out of this crate. If an automation command needs shared domain data, depend on the smallest domain crate that owns that data.

Validation:

```sh
cargo run -p xtask -- generate-keybindings-doc --check
cargo run -p xtask -- generate-config-doc --check
```
