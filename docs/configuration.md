# Configuration

Termy uses a text config file inspired by Ghostty:

- `~/.config/termy/config.txt`

## Recommended Starter Config

Most users only need this:

```txt
theme = termy
term = xterm-256color
use_tabs = true
tab_title_mode = smart
tab_title_shell_integration = true
```

## Terminal Runtime

### Basic (recommended)

`term`
- Default: `xterm-256color`
- Values: terminal type string (for example `xterm-256color`, `screen-256color`)
- What it does: sets `TERM` for child shells/apps. Keep the default unless you have a specific compatibility need.

### Advanced (optional)

`shell`
- Default: unset (uses your login shell from env; fallback is platform-specific)
- Values: absolute executable path (for example `/bin/zsh`)
- What it does: forces the shell used for new terminal sessions.

`working_dir_fallback`
- Default: `home` on macOS/Windows, `process` on Linux
- Values: `home`, `process`
- What it does: startup directory used only when `working_dir` is unset.

`colorterm`
- Default: `truecolor`
- Values: string value or `none`/`unset`/`default`/`auto` to disable
- What it does: sets `COLORTERM` for child apps (usually `truecolor` for modern color support).

## Keybindings

Keybinding syntax, defaults, and override examples now live in:

- `docs/keybindings.md`

`command_palette_show_keybinds`
- Default: `true`
- Values: `true`/`false`
- What it does: shows shortcut badges on the right side of command palette command rows.

## Tab Titles

### Basic (recommended)

`tab_title_mode`
- Default: `smart`
- Values: `smart`, `shell`, `explicit`, `static`
- What it does: chooses a sensible title strategy.

Mode presets:
- `smart`: `manual, explicit, shell, fallback`
- `shell`: `manual, shell, fallback`
- `explicit`: `manual, explicit, fallback`
- `static`: `manual, fallback`

`tab_title_shell_integration`
- Default: `true`
- Values: `true`/`false`
- What it does: exports `TERMY_*` environment variables for shell hooks.

`tab_title_fallback`
- Default: `Terminal`
- Values: non-empty string
- What it does: fallback title if higher-priority sources are empty.

Note:
- Termy applies a built-in short delay before showing `command:...` titles to reduce flash for fast commands.

### Advanced (optional)

`tab_title_priority`
- Default: unset (derived from `tab_title_mode`)
- Values: comma-separated list using `manual`, `explicit`, `shell`, `fallback`
- What it does: exact source order override. If set, it wins over `tab_title_mode`.

`tab_title_explicit_prefix`
- Default: `termy:tab:`
- Values: string prefix
- What it does: marks explicit payloads in OSC title updates.

`tab_title_prompt_format`
- Default: `{cwd}`
- Values: template string with optional `{cwd}` and `{command}` placeholders
- What it does: formats explicit `prompt:...` payloads.

`tab_title_command_format`
- Default: `{command}`
- Values: template string with optional `{cwd}` and `{command}` placeholders
- What it does: formats explicit `command:...` payloads.

Explicit payload examples:
- `termy:tab:prompt:~/projects/termy`
- `termy:tab:command:cargo test`
- `termy:tab:title:Deploy`

## All Config Options

`theme`
- Default: `termy`
- Values: `termy`, `tokyonight`, `catppuccin`, `dracula`, `gruvbox`, `nord`, `solarized`, `onedark`, `monokai`, `material`, `palenight`, `tomorrow`, `oceanic`
- Tip: command palette `Switch Theme` updates this value and persists it to config.

`working_dir`
- Default: unset
- Values: path string (`~` supported)

`working_dir_fallback`
- Default: `home` on macOS/Windows, `process` on Linux
- Values: `home`, `process`

`shell`
- Default: unset
- Values: executable path string

`term`
- Default: `xterm-256color`
- Values: terminal type string

`colorterm`
- Default: `truecolor`
- Values: string, or `none`/`unset`/`default`/`auto` to disable

`use_tabs`
- Default: `false`
- Values: `true`/`false`

`tab_title_mode`
- Default: `smart`
- Values: `smart`, `shell`, `explicit`, `static`

`tab_title_shell_integration`
- Default: `true`
- Values: `true`/`false`

`tab_title_fallback`
- Default: `Terminal`
- Values: non-empty string

`tab_title_priority`
- Default: unset (derived from `tab_title_mode`)
- Values: `manual`, `explicit`, `shell`, `fallback` (comma-separated)

`tab_title_explicit_prefix`
- Default: `termy:tab:`
- Values: string

`tab_title_prompt_format`
- Default: `{cwd}`
- Values: template string

`tab_title_command_format`
- Default: `{command}`
- Values: template string

`window_width`
- Default: `1280`
- Values: positive number

`window_height`
- Default: `820`
- Values: positive number

`font_family`
- Default: `JetBrains Mono`
- Values: font family name

`font_size`
- Default: `14`
- Values: positive number

`cursor_style`
- Default: `block`
- Values: `block`, `line` (`bar`/`beam`/`ibeam` are accepted aliases for `line`)
- What it does: sets one shared cursor shape for the terminal grid and GPUI inline inputs (command palette + tab rename).

`cursor_blink`
- Default: `true`
- Values: `true`/`false`
- What it does: enables/disables cursor blinking for both terminal and inline inputs.

`background_opacity`
- Default: `1.0`
- Values: number between `0.0` and `1.0`
- What it does: controls whole-window background transparency (`0.0` fully transparent, `1.0` opaque).

`background_blur`
- Default: `false`
- Values: `true`/`false`
- What it does: requests platform blur for transparent backgrounds.
- Note: blur strength is not configurable in v1; this is on/off only.
- Note: support depends on platform/session/compositor.

`padding_x`
- Default: `12`
- Values: non-negative number

`padding_y`
- Default: `8`
- Values: non-negative number

`mouse_scroll_multiplier`
- Default: `3`
- Values: any finite number (clamped to `0.1..=1000`)
- What it does: multiplies mouse wheel scroll distance. For example, `3` scrolls about three lines per wheel tick.

`scrollbar_visibility`
- Default: `on_scroll`
- Values: `always`, `on_scroll`, `off`
- What it does: controls terminal viewport scrollbar visibility behavior.
- `off`: hide the scrollbar unless you are scrolled up in history.
- `always`: always show the terminal scrollbar when scrollback exists.
- `on_scroll`: show while scrolling/dragging, then auto-hide after inactivity.
- Note: when you are scrolled up in history, the scrollbar stays visible in all modes.

`scrollbar_style`
- Default: `neutral`
- Values: `neutral`, `muted_theme`, `theme`
- What it does: controls scrollbar colors.
- `neutral`: keep the scrollbar in neutral gray.
- `muted_theme`: blend theme background and accent for a softer, integrated tint.
- `theme`: use the direct theme accent color.
- Applies to both terminal viewport scrollbar and command palette/theme switcher scrollbar so they stay visually consistent.

`keybind`
- Default: built-in platform shortcuts
- Values: repeated `keybind` directives (see `docs/keybindings.md`)

`command_palette_show_keybinds`
- Default: `true`
- Values: `true`/`false`

## Custom Colors

Override individual theme colors using a `[colors]` section. All colors are hex format (`#RRGGBB`).

### Basic Example

```txt
theme = termy

[colors]
foreground = #e7ebf5
background = #0b1020
cursor = #a7e9a3
```

### All Color Keys

| Key | Alias | Description |
|-----|-------|-------------|
| `foreground` | `fg` | Default text color |
| `background` | `bg` | Terminal background |
| `cursor` | - | Cursor color |
| `black` | `color0` | ANSI black |
| `red` | `color1` | ANSI red |
| `green` | `color2` | ANSI green |
| `yellow` | `color3` | ANSI yellow |
| `blue` | `color4` | ANSI blue |
| `magenta` | `color5` | ANSI magenta |
| `cyan` | `color6` | ANSI cyan |
| `white` | `color7` | ANSI white |
| `bright_black` | `color8` | ANSI bright black |
| `bright_red` | `color9` | ANSI bright red |
| `bright_green` | `color10` | ANSI bright green |
| `bright_yellow` | `color11` | ANSI bright yellow |
| `bright_blue` | `color12` | ANSI bright blue |
| `bright_magenta` | `color13` | ANSI bright magenta |
| `bright_cyan` | `color14` | ANSI bright cyan |
| `bright_white` | `color15` | ANSI bright white |

### Full Example

```txt
theme = termy

[colors]
foreground = #e7ebf5
background = #0b1020
cursor = #a7e9a3

# Normal colors
black = #0b1020
red = #f1b8c5
green = #a7e9a3
yellow = #f7dba0
blue = #a3c5f7
magenta = #d3b8f0
cyan = #99e5eb
white = #d5d9e5

# Bright colors
bright_black = #4a4e5a
bright_red = #ffb8b8
bright_green = #b8ffb8
bright_yellow = #fffab8
bright_blue = #b8d4ff
bright_magenta = #e5b8ff
bright_cyan = #b8ffff
bright_white = #ffffff
```

### Importing Colors from JSON

Use command palette actions:

- `Switch Theme` to pick and persist a theme quickly
- `Import Colors` to import a `[colors]` override from JSON

JSON format for `Import Colors`:

```json
{
  "foreground": "#e7ebf5",
  "background": "#0b1020",
  "cursor": "#a7e9a3",
  "black": "#0b1020",
  "red": "#f1b8c5",
  "green": "#a7e9a3"
}
```

Keys starting with `$` are ignored (useful for JSON schema references).

## Shell Integration Snippets

If `tab_title_shell_integration = true`, Termy exports:

- `TERMY_SHELL_INTEGRATION=1`
- `TERMY_TAB_TITLE_PREFIX=<tab_title_explicit_prefix>`

### zsh (`~/.zshrc`)

```sh
if [[ "${TERMY_SHELL_INTEGRATION:-0}" == "1" ]]; then
  _termy_emit_tab_title() {
    local kind="$1"
    shift
    local payload="$*"
    local prefix="${TERMY_TAB_TITLE_PREFIX:-termy:tab:}"
    payload=${payload//$'\n'/ }
    payload=${payload//$'\r'/ }
    printf '\033]2;%s%s:%s\007' "$prefix" "$kind" "$payload"
  }

  _termy_prompt_title() {
    _termy_emit_tab_title "prompt" "${PWD/#$HOME/~}"
  }

  _termy_command_title() {
    _termy_emit_tab_title "command" "$1"
  }

  autoload -Uz add-zsh-hook
  add-zsh-hook precmd _termy_prompt_title
  add-zsh-hook preexec _termy_command_title
fi
```

### bash (`~/.bashrc`)

```sh
if [[ "${TERMY_SHELL_INTEGRATION:-0}" == "1" ]]; then
  _termy_emit_tab_title() {
    local kind="$1"
    shift
    local payload="$*"
    local prefix="${TERMY_TAB_TITLE_PREFIX:-termy:tab:}"
    payload=${payload//$'\n'/ }
    payload=${payload//$'\r'/ }
    printf '\033]2;%s%s:%s\007' "$prefix" "$kind" "$payload"
  }

  _termy_preexec() {
    [[ "$BASH_COMMAND" == "$PROMPT_COMMAND" ]] && return
    _termy_emit_tab_title "command" "$BASH_COMMAND"
  }

  _termy_precmd() {
    _termy_emit_tab_title "prompt" "${PWD/#$HOME/~}"
  }

  trap '_termy_preexec' DEBUG
  PROMPT_COMMAND="_termy_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
```

### fish (`~/.config/fish/config.fish`)

```fish
if test "$TERMY_SHELL_INTEGRATION" = "1"
  function __termy_emit_tab_title
    set kind $argv[1]
    set payload (string join " " $argv[2..-1])
    set payload (string replace -a \n " " $payload)
    set payload (string replace -a \r " " $payload)
    set prefix (set -q TERMY_TAB_TITLE_PREFIX; and echo $TERMY_TAB_TITLE_PREFIX; or echo "termy:tab:")
    printf '\e]2;%s%s:%s\a' $prefix $kind $payload
  end

  function __termy_preexec --on-event fish_preexec
    __termy_emit_tab_title command $argv
  end

  function __termy_prompt --on-event fish_prompt
    set cwd (string replace -r "^$HOME" "~" $PWD)
    __termy_emit_tab_title prompt $cwd
  end
end
```
