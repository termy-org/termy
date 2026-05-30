# termy_release_core

Release metadata and version helpers.

This crate owns shared release/update metadata parsing and version comparison logic used by the CLI and updater. It should not own installer execution, UI, or platform packaging scripts.

Use this crate when changing how Termy understands releases, versions, or downloadable artifacts.

Validation:

```sh
cargo test -p termy_release_core
```
