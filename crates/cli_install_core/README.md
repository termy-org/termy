# termy_cli_install_core

Shared CLI installation helpers.

This crate owns path resolution and filesystem helpers used to install or locate Termy's command-line tools. It must stay independent of GPUI and desktop app state.

Use this crate when install behavior needs to be reused by the desktop app and `termy-cli`.

Validation:

```sh
cargo test -p termy_cli_install_core
```
