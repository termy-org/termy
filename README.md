# Termy

A fast, minimal terminal emulator built with [GPUI](https://gpui.rs) and [alacritty_terminal](https://alacritty.org).

[Docs](https://termy.run/docs) · [Download](https://termy.run/#download) · [Contribute](CONTRIBUTING.md)

## Features

- GPU-accelerated rendering with dirty-span cell caching
- Tabs, splits, and search
- Configurable keybinds and themes
- Out-of-process plugin system (stdio JSON protocol)
- Native OS integrations on macOS

## Install

Prebuilt binaries: [termy.run](https://termy.run/#download).

Build from source:

```bash
cargo run --release -p termy
```

## Configuration

Config and keybinds live under your platform config dir. See [docs/configuration.md](docs/configuration.md) and [docs/keybindings.md](docs/keybindings.md).

## Platform Notes

- Windows: Agent Sidebar/Workspace is currently disabled.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, build, and validation commands.

## License

MIT. See [LICENSE](LICENSE).
