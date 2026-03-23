---
title: Customization Guide
description: Personalize fonts, theme, opacity, and keybindings
order: 1
category: Guides
---

Use this guide to make Termy feel like your terminal.

## Appearance basics

Set your preferred font and size in `~/.config/termy/config.txt`:

```txt
font_family = JetBrains Mono
font_size = 14
theme = termy
```

## Window style

You can tune opacity and blur:

```txt
background_opacity = 0.95
background_blur = true
padding_x = 12
padding_y = 8
```

## Cursor and scrolling

```txt
cursor_style = block
cursor_blink = true
scrollback_history = 5000
mouse_scroll_multiplier = 3
```

## Keybinding overrides

Override a shortcut with `keybind` lines:

```txt
keybind = secondary-p=toggle_command_palette
keybind = secondary-t=new_tab
```

For full action and syntax details, see [Keybindings](/docs/keybindings).

## Keep config changes safe

Make one change at a time. If startup breaks, restore your backup config and retry.
