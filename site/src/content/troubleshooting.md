---
title: Troubleshooting
description: Fix common install and launch problems
order: 1
category: Help & Troubleshooting
---

## macOS: "Termy is damaged" or blocked by Gatekeeper

Termy is not code signed yet. Remove quarantine after moving the app to `Applications`:

```bash
sudo xattr -d com.apple.quarantine /Applications/Termy.app
```

Then reopen Termy.

## Windows: SmartScreen warning

If Windows shows a security prompt:

1. Click `More info`
2. Click `Run anyway`

## Linux: AppImage does not start

1. Make sure it is executable:

```bash
chmod +x Termy-*.AppImage
```

2. Run it directly from a terminal to inspect any runtime error:

```bash
./Termy-*.AppImage
```

## `termy` command not found

If you installed to `~/.local/bin`, ensure that directory is in your shell `PATH`.

## Reset local config

If a config change broke startup, move your config out of the way and restart:

```bash
mv ~/.config/termy/config.txt ~/.config/termy/config.txt.bak
```
