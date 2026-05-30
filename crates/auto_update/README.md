# termy_auto_update

Update-checking and installer handoff logic for Termy.

This crate owns release discovery, artifact verification, platform update decisions, and OS handoff points. It may depend on `termy_release_core` for release metadata, but it should not own desktop rendering or update UI.

Use this crate when changing how Termy finds, validates, or launches updates.

Validation:

```sh
cargo test -p termy_auto_update
```
