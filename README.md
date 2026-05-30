# Termy

A fast, minimal terminal emulator built with [GPUI](https://gpui.rs) and [alacritty_terminal](https://alacritty.org).

[Docs](https://termy.sh/docs) · [Download](https://termy.sh/#download) · [Contribute](CONTRIBUTING.md)

## Features

- GPU-accelerated rendering with dirty-span cell caching
- Tabs, splits, and search
- Configurable keybinds and themes
- Tasks, layouts, and optional tmux sessions
- Native OS integrations on macOS

## Install

Prebuilt binaries: [termy.sh](https://termy.sh/#download).

Build from source:

```bash
cargo run --release -p termy
```

## Configuration

Config and keybinds live under your platform config dir. See [docs/configuration.md](docs/configuration.md) and [docs/keybindings.md](docs/keybindings.md).

## Architecture

Termy is a Rust workspace with a GPUI desktop app, reusable headless runtime, CLI, FFI, website, and platform packaging scripts. See [Project Layout](docs/architecture/project-layout.md) for ownership boundaries and [Release Packaging](docs/architecture/release-packaging.md) for release artifact flow.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, build, and validation commands.

## License

MIT. See [LICENSE](LICENSE).
