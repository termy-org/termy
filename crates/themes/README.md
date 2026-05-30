# termy_themes

Bundled Termy themes.

This crate owns built-in theme definitions and their registration. It should depend on `termy_theme_core` for the data model and stay independent of GPUI and app config I/O.

Use this crate when adding or changing bundled color themes.

Validation:

```sh
cargo test -p termy_themes
```
