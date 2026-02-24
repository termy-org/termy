<div align="center">
  <img src="assets/termy_icon@1024px.png" width="120" alt="Termy icon" />
  <h1>Termy</h1>
  <p>A minimal terminal emulator built with <a href="https://gpui.rs">GPUI</a> and <a href="https://alacritty.org">alacritty_terminal</a>.</p>
</div>

---

## Installation

### macOS — Homebrew (recommended)

```sh
brew tap lassejlv/termy https://github.com/lassejlv/termy
brew install --cask termy
```

### macOS — Direct download

Download the latest `.dmg` from the [Releases](https://github.com/lassejlv/termy/releases/latest) page:

- **Apple Silicon (arm64):** `Termy-<version>-macos-arm64.dmg`
- **Intel (x86_64):** `Termy-<version>-macos-x86_64.dmg`

### Linux

Download the latest tarball from the [Releases](https://github.com/lassejlv/termy/releases/latest) page:

- `Termy-<version>-linux-x86_64.tar.gz`

Extract and run the binary:

```sh
tar -xzf Termy-*-linux-x86_64.tar.gz
./termy
```

### Arch Linux

```sh
paru -S termy-bin
```

### Windows

Download the latest installer from the [Releases](https://github.com/lassejlv/termy/releases/latest) page:

- `Termy-<version>-x64-Setup.exe`

Run the installer and follow the setup wizard.

### Build from source

> Requires Rust (stable).

```sh
cargo run --release
```

## Configuration

Config file: `~/.config/termy/config.txt`

```txt
theme = termy
font_family = "JetBrains Mono"
font_size = 14
window_width = 1100
window_height = 720
keybind = cmd-p=toggle_command_palette
```

See [`docs/configuration.md`](docs/configuration.md) and [`docs/keybindings.md`](docs/keybindings.md) for the full reference.

## Building packages

| Platform | Command |
|----------|---------|
| macOS DMG | `./scripts/build-dmg.sh` |
| macOS signed DMG | `./scripts/build-dmg-signed.sh --sign-identity "..." --notary-profile TERMY_NOTARY` |
| Windows Setup.exe | `./scripts/build-setup.ps1 -Version 0.1.0 -Arch x64 -Target x86_64-pc-windows-msvc` |

## License

MIT — see [LICENSE](LICENSE).
