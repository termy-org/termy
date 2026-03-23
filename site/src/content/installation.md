---
title: Installation
description: Install Termy on macOS, Windows, or Linux with first-launch steps
order: 0
category: Getting Started
---

This guide gets you from download to first terminal prompt.

## Download Termy

Use the latest release from GitHub and pick the asset for your platform.

## macOS

1. Download the `.dmg` file.
2. Open it and drag `Termy.app` into `Applications`.
3. Launch Termy from `Applications`.

Because Termy is not code signed yet, macOS may block first launch. If that happens, run:

```bash
sudo xattr -d com.apple.quarantine /Applications/Termy.app
```

Then launch Termy again.

## Windows

1. Download the `Setup.exe` asset.
2. Run the installer.
3. Start Termy from the Start menu.

Because Termy is not code signed yet, Windows SmartScreen may warn on first launch. Click:

1. `More info`
2. `Run anyway`

## Linux

### AppImage

1. Download the `.AppImage` asset.
2. Make it executable:

```bash
chmod +x Termy-*.AppImage
```

3. Run it:

```bash
./Termy-*.AppImage
```

### Tarball

1. Download the `.tar.gz` asset.
2. Extract it:

```bash
tar -xzf Termy-*.tar.gz
```

3. Run `termy/termy` or install with the included script:

```bash
cd termy
./install.sh
```

### Arch Linux

```bash
paru -S termy-bin
```

## Verify Installation

```bash
termy --version
```

If `termy` is not found, add your install directory to `PATH` (commonly `~/.local/bin`).
