# termy_config_core

Shared configuration schema and defaults.

This crate owns Termy's config data model, defaults, validation-friendly types, and theme references used by app, CLI, docs, and embedding surfaces.

Keep terminal runtime behavior in `termy_core`, command metadata in `termy_command_core`, and bundled theme definitions in `termy_themes`.

Validation:

```sh
cargo test -p termy_config_core
just check-config-doc
```
