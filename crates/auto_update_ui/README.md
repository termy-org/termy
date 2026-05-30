# termy_auto_update_ui

User-facing update UI wrappers.

This crate owns small UI-facing update components that sit above `termy_auto_update`. Keep release fetching and verification in `termy_auto_update`; keep full app workflows in `crates/desktop_app/`.

Use this crate when update behavior needs a reusable UI surface.

Validation:

```sh
cargo test -p termy_auto_update_ui
```
