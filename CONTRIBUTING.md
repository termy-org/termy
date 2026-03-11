# Contributing to Termy

Thanks for contributing.

This guide focuses on the current Termy workflow so you can get a change from clone to PR without guessing.

## Before you start

- Check open issues and PRs for overlap before starting a larger change.
- Keep changes scoped. Small, reviewable PRs move faster here.
- If you touch generated docs, regenerate them instead of editing generated files by hand.

## Project layout

- `src/`: main desktop app built with Rust + GPUI
- `crates/`: shared workspace crates such as config, command catalog, plugin host, search, terminal UI, and CLI helpers
- `docs/`: repository docs used by contributors
- `site/`: website and public docs
- `.github/`: issue templates, PR template, and CI workflows

## Local setup

Requirements:

- Rust stable toolchain
- Platform build dependencies for GPUI and native packaging where relevant

Optional tools:

- `just` for the repo command shortcuts
- `cargo-watch` if you want the `just dev` loop
- `tmux >= 3.3` if you want to run tmux integration tests

Clone and verify the workspace:

```sh
cargo check --workspace
```

## Common commands

Run the app:

```sh
cargo run -p termy
```

Useful `just` commands:

```sh
just
just check
just dev
just test-tmux-integration
just check-boundaries
```

## Docs and generated files

These files are generated. Do not edit them manually:

- `docs/keybindings.md`
- `docs/configuration.md`

Regenerate them with:

```sh
just generate-keybindings-doc
just generate-config-doc
```

Or verify they are up to date with:

```sh
just check-keybindings-doc
just check-config-doc
```

## Testing and validation

Use the smallest validation pass that proves your change.

Common options:

```sh
cargo check -p termy
cargo test -p termy_config_core
just check-boundaries
```

If your change touches tmux behavior, also run:

```sh
just test-tmux-integration
```

## Config and command changes

If you change config keys:

- update the config schema in `crates/config_core`
- keep parsing, defaults, and rendering in sync
- regenerate config docs

If you change commands or keybind-facing actions:

- update the command catalog in `crates/command_core`
- wire the action through the app in `src/`
- regenerate keybinding docs if defaults or public command names changed

## Documentation changes

Pick the right place:

- contributor-facing repo docs: `docs/`
- public user docs: `site/src/content/`
- quick project entry points: `README.md`

When behavior changes, update docs in the same PR.

## Pull requests

Before opening a PR:

- make sure the branch is up to date enough to review cleanly
- run the relevant checks for your change
- fill out `.github/PULL_REQUEST_TEMPLATE.md`
- link the issue with `Closes #<number>` when appropriate

Good PRs usually include:

- what changed
- why it changed
- any user-visible behavior changes
- screenshots or video for UI work
- notes about tests or intentionally skipped validation

## Style expectations

- Prefer small, explicit code paths over clever abstractions.
- Preserve existing project structure and naming where possible.
- Keep docs and behavior aligned.
- Avoid unrelated drive-by changes in the same PR.

## Questions

If the right direction is unclear, open an issue first or start with a smaller draft PR that frames the change.
