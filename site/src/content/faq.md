---
title: FAQ
description: Frequently asked questions
order: 2
category: Help & Troubleshooting
---

## Is Termy free?

Yes. Termy is free and open source.

## Is Termy signed/notarized?

Not yet. On first launch:

- macOS users may need:

```bash
sudo xattr -d com.apple.quarantine /Applications/Termy.app
```

- Windows users may need to click `More info` then `Run anyway`.

## Where is the config file?

`~/.config/termy/config.txt`

## How do I install on Arch Linux?

```bash
paru -S termy-bin
```

## How do I update?

Open the Command Palette and run `Check for updates`.

- macOS: `cmd+p`
- Windows/Linux: `ctrl+p`
