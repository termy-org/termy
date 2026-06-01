# `terminal_view/` decomposition plan

**Problem:** `crates/desktop_app/src/terminal_view/mod.rs` (~5.5k lines) and `render.rs` (~4k lines) concentrate behavior, review cost, and merge conflicts.

**Goal:** No file above **1,500** lines by v1.0; no new file above **800** lines without an ADR. Median tab/render PR touches ≤3 files.

**Non-goal:** A single “big bang” rewrite. Extract by **vertical slice** with tests moved or added per slice.

---

## Ownership map (target end state)

| Module / directory | Owns | Must not own |
|--------------------|------|--------------|
| `tabs/` | Tab model, lifecycle, drag, sizing, persistence hooks | Grid paint, tmux protocol |
| `tab_strip/` | Chrome, layout, gestures, titlebar drag | Terminal cell cache |
| `interaction/` | Input, selection, scroll, mouse, context menu | Config schema |
| `runtime/` | App runtime loop, tmux sync, session coordination | Settings UI |
| `runtime/tmux/` | App-side tmux actions/events | Low-level tmux client (`terminal_ui`) |
| `command_palette/` | Palette state and layout | Command catalog definitions (`command_core`) |
| `render/` *(new)* | Paint orchestration, dirty spans, layer composition | Tab lifecycle |
| `search.rs` | In-view search UI wiring | Search algorithm (`search` crate) |

`mod.rs` should shrink to: struct definition, trait impls glue, `impl TerminalView` dispatch, and `mod` declarations.

---

## Extraction tranches

Execute **one tranche per release** (or per month), each ≤500 lines moved, with `cargo test -p termy` green.

### Tranche 1 — Session & window glue (E1 Q2)

- Move window-scoped session wiring out of `mod.rs` → `window_appearance.rs` (started in #319).
- **Exit:** `mod.rs` −300 lines; no behavior change; tests unchanged count.
- **Progress:** `window_appearance.rs` holds background blur/opacity + system appearance handlers (~196 lines removed from `mod.rs`).

### Tranche 2 — Tab lifecycle boundary (E1 Q2–Q3)

- Ensure all open/close/select/pin paths live under `tabs/lifecycle.rs` (+ siblings).
- `mod.rs` only forwards to `tabs::*`.
- **Exit:** No tab lifecycle `fn` bodies remain in `mod.rs`.

### Tranche 3 — Input routing (E1 Q3)

- Centralize event dispatch in `interaction/dispatch.rs` (new) or expand `interaction/input.rs`.
- **Exit:** `mod.rs` does not match on raw key/mouse enums.

### Tranche 4 — Render split (E1 Q3–Q4)

- Create `render/` directory:
  - `paint.rs` — grid paint, cell cache, dirty spans
  - `layers.rs` — overlays (search, scrollbar, hints)
  - `mod.rs` — public `render_*` entrypoints used by `TerminalView`
- Slim current `render.rs` into the directory over 2–3 PRs.
- **Exit:** `render.rs` deleted; largest file in `render/` &lt; 1,200 lines.

### Tranche 5 — Tmux app orchestration (E1 Q4)

- Collapse tmux-specific branches from `mod.rs` into `runtime/tmux/` (already started).
- **Exit:** `mod.rs` &lt; 1,500 lines total.

---

## Enforcement (E1.1)

Add `scripts/check-file-sizes.sh` (or `xtask check-file-sizes`):

- Fail CI if any tracked `crates/**/*.rs` exceeds **1,500** lines.
- Warn (or fail) if a PR **adds** a new file over **800** lines.
- Allowlist file paths with issue links until tranche completes (shrink allowlist over time).

Initial allowlist (see `scripts/check-file-sizes.sh` — shrink as tranches land):

- `terminal_view/mod.rs`, `terminal_view/render.rs` (tranches 1–4)
- `terminal_ui/src/grid.rs`, `core/src/runtime.rs`
- Grandfathered until split: `command_palette/mod.rs`, `inline_input.rs`, `tabs/lifecycle.rs`, `settings_view/sections.rs`, `ffi/src/lib.rs`, `xtask/src/benchmark.rs`

---

## Product dependencies

| Product roadmap item | Decomposition dependency |
|----------------------|---------------------------|
| Multiple windows | Tranche 1 (session/window glue) |
| MRU tabs | Tranche 2 |
| OSC / rendering changes | Tranche 4 (render/) |
| Large scrollback perf | Tranche 4 + benchmarks (E3) |

Do **not** block Phase 1 ship blockers on decomposition. **Do** require tranche 1 before multi-window beta.

---

## Related

- [Engineering roadmap](roadmap.md) — phase E1
- [Project layout](../architecture/project-layout.md)
