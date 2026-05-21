# Termy v1.0 Roadmap

> Current version: **0.1.72** | 89k lines of Rust | 1,008 tests | 3-platform CI | 91 issues closed

---

## Phase 1 -- Release Blockers

These must be resolved before v1 ships.

### Distribution & Trust

| Item | Platform | Reference |
|------|----------|-----------|
| macOS code signing + notarization | macOS | [#225](https://github.com/termy-org/termy/issues/225) |
| Windows code signing (EV certificate) | Windows | -- |
| Replace placeholder bundle identifier (`com.example.termy`) | All | -- |

### Critical Bugs

| Item | Platform | Reference |
|------|----------|-----------|
| Theme deeplink not working on Windows | Windows | [#288](https://github.com/termy-org/termy/issues/288) |
| Settings window close button broken on Appearance tab | Windows | [#281](https://github.com/termy-org/termy/issues/281) |

### Terminal Correctness

| Item | Platform | Reference |
|------|----------|-----------|
| Proper OSC sequence support | All | [#149](https://github.com/termy-org/termy/issues/149) |

---

## Phase 2 -- Platform Parity

Close the gaps between macOS and the other platforms.

### Windows

- Ship agent sidebar on Windows (currently stubbed out in `agents_windows.rs`)
- Harden auto-update, deeplink, and installer flows in CI

### Linux

- Implement right-click context menus (currently returns `None` on all entries)
- Add file drop support (currently macOS-only)
- Add ARM64 Linux builds to CI
- Publish to Flatpak/Flathub and Copr for discoverability

---

## Phase 3 -- Feature Completion

Features expected from a v1 terminal emulator.

### Core

| Item | Reference |
|------|-----------|
| Multiple window support (`Cmd+N` / `Ctrl+Shift+N`) | -- |
| MRU tab switching | [#240](https://github.com/termy-org/termy/issues/240) |
| Ghostty config compatibility | [#290](https://github.com/termy-org/termy/issues/290) |
| Font ligature support | -- |
| Sixel / image protocol support | -- |

### Agent Workspace

| Item | Reference |
|------|-----------|
| Collect and act on agent workspace feedback | [#286](https://github.com/termy-org/termy/issues/286) |
| Remove or implement the empty `crates/agents` crate | -- |

---

## Phase 4 -- Production Hardening

What separates a beta from a stable release.

### Stability

- Add crash reporting (at minimum, write a crash log file on panic)
- Replace `.unwrap()` on startup window creation with a graceful error dialog
- Stress test: rapid tab open/close, large scrollback, concurrent tmux sessions

### Performance

- Establish a CI benchmark suite for startup time, input latency, and scroll performance
- Validate large scrollback (100k+ lines) does not degrade

### Accessibility

- Add screen reader labels and keyboard-only navigation
- Ensure themes respect OS-level high contrast settings

### Cleanup

- Sync sub-crate versions (all at `0.1.0` while app is `0.1.72`)
- Extract `termy_api` to its own repository to reduce workspace dependency weight

---

## Phase 5 -- Launch

The final push before tagging v1.0.

- Write user-facing documentation: config reference, keybinds, theme authoring, CLI usage
- Publish migration guides for users switching from Ghostty, Alacritty, iTerm2, and Windows Terminal
- Compile a v1 changelog from all closed issues and unreleased work
- Run a dependency license audit
- Polish termy.sh to reflect v1 quality

---

## Priority Summary

| Priority | Scope | Key Items |
|----------|-------|-----------|
| **P0** | Ship blockers | Code signing, bundle ID, OSC support, open bugs |
| **P1** | Platform parity | Windows agent sidebar, Linux context menus, multi-window, crash reporting |
| **P2** | Adoption | MRU tabs, Ghostty compat, Linux packaging, benchmarks |
| **P3** | Completeness | Accessibility, image protocols, ligatures, docs |

---

## Already Solid

These areas are production-ready and do not need significant work:

- Tabbed terminal with drag-to-reorder and pinning
- tmux integration (split panes, session management)
- Theme store with deeplink install
- Auto-update pipeline
- Configuration system with diagnostics and live-reload
- Command palette
- In-terminal search with scrollbar markers
- Settings UI
- CLI companion tool
- Toast feedback system
