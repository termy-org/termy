---
title: First Steps
description: Quick onboarding after install
order: 3
category: Getting Started
---

Once Termy is installed, use this checklist to get productive quickly.

## 1. Open the Command Palette

Use `secondary-p` to open the command palette.

`secondary` means:
- `cmd` on macOS
- `ctrl` on Windows/Linux

## 2. Learn the core tab shortcuts

- New tab: `secondary-t`
- Close tab: `secondary-w`
- Search: `secondary-f`

Full shortcut list: [Keybindings](/docs/keybindings)

## 3. Configure your shell and theme

Edit `~/.config/termy/config.txt` and set a few basics:

```txt
font_family = JetBrains Mono
font_size = 14
theme = termy
```

Full config reference: [Configuration](/docs/configuration)

## 4. Optional: install from a package manager

- macOS: Homebrew cask
- Arch Linux: `paru -S termy-bin`

## 5. Keep Termy updated

Open the Command Palette with:

- `cmd+p` on macOS
- `ctrl+p` on Windows/Linux

Then run `Check for updates`.
