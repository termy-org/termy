# Termy Performance Improvement Plan

## Executive Summary

This document outlines a comprehensive plan to improve performance across all aspects of Termy — a GPUI-based terminal emulator. The plan targets rendering performance, memory efficiency, input latency, and startup time.

**Current State:**
- Built on GPUI (Zed's UI framework) and `alacritty_terminal`
- Full benchmark infrastructure in `crates/xtask/src/benchmark.rs` (driver, compare, scenario runner)
- Atomic render metrics: `grid_paint_count`, `shape_line_calls`, `shaped_line_cache_hits/misses`, `runtime_wakeup_count` (`render_metrics.rs`)
- Row-level damage tracking with `TerminalGridPaintDamage::{None, Full, Rows}`
- Per-row `ShapedLine` cache in `CachedRowPaintOps`, with shaped-line reuse across matching rows
- Text batching by `(bold, fg, underline)` within each row (`TextBatch` in `grid.rs`)
- Background span batching by color per row (`BackgroundSpan`)
- Unicode block element rendering as pixel-snapped quads (no glyph rasterization)
- Tmux notification coalescing with per-pane output merging and backpressure (`coalescer.rs`)

**Architectural constraints to keep in mind:**
- GPUI owns the GPU text atlas and text shaping pipeline. Custom atlas work must go through `window.text_system()`, not around it.
- Scrollback buffer and `Cell` allocation live inside `alacritty_terminal::Term`. Changes there require patching or forking the dependency.
- Input dispatch is owned by GPUI's event loop. Bypassing it for a custom input thread requires forking GPUI.

**Target Metrics:**
- Maintain 60 FPS during heavy terminal output (>10 MB/s)
- <16ms input latency (current target for perceived responsiveness)
- <200ms cold startup time
- <100MB base memory footprint

---

## Phase 1: Rendering Pipeline Optimization

### 1.1 Cell-Level Damage Tracking (High Impact)

**Current:** `TerminalGridPaintDamage::Rows(Arc<[usize]>)` marks entire rows dirty. A single cursor blink or character update invalidates and re-shapes the whole row.

**Solution:** Add cell-level and region-level damage variants.

```rust
// In crates/terminal_ui/src/grid.rs
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TerminalGridPaintDamage {
    #[default]
    None,
    Full,
    Rows(Arc<[usize]>),
    // NEW: sparse cell updates (e.g. single character change)
    Cells(Arc<[(usize, usize)]>), // (row, col) pairs
    // NEW: contiguous rectangular region (e.g. scroll region)
    Region { start_row: usize, end_row: usize, start_col: usize, end_col: usize },
}
```

**Implementation:**
1. Extend `paint_damage_from_dirty_spans` to emit `Cells` when the dirty span covers ≤4 columns
2. Detect contiguous multi-row dirty spans and emit `Region` instead of `Rows`
3. In `dirty_rows_for_pass`, handle cursor movement as a `Cells` update (only the two cursor cells, not full rows)
4. In `rebuild_cached_rows_for_pass`, skip re-shaping unchanged cells within a dirty row

**Expected Gain:** 20-40% reduction in `shape_line` calls during typical editing and cursor movement

### 1.2 Cursor Blink as Overlay (High Impact)

**Current:** Cursor blink invalidates the cursor row via `dirty_rows_for_pass`, causing the entire row's text to be re-shaped and repainted.

**Solution:** Track cursor as a separate overlay painted last, so blink only repaints the cursor quad — not the row.

**Implementation:**
1. Remove cursor from `dirty_rows_for_pass` cursor-transition logic when only blink state changed (no position change)
2. Paint cursor quad in a separate pass after all row ops, using the already-cached row ops
3. Store `last_cursor_blink_state: bool` in `TerminalGridPaintCache` to detect blink-only transitions

**Expected Gain:** Eliminates row re-shape on every cursor blink tick (~60 unnecessary `shape_line` calls/sec)

### 1.3 Reduce `shape_line` Calls for Identical Rows (Medium Impact)

**Current:** `find_matching_previous_row_ops_index` reuses `ShapedLine` objects from matching previous rows. However, it only checks ±1 row neighbors first, then does a linear scan. For large terminals with many identical rows (e.g. blank lines), this scan is O(n).

**Solution:** Index previous row ops by a cheap content hash for O(1) lookup.

```rust
// In TerminalGridPaintCache
shaped_line_index: HashMap<u64, usize>, // content_hash -> row index
```

**Implementation:**
1. Compute a cheap hash of `(background_spans, draw_ops text/style)` when building `CachedRowPaintOps`
2. Store in `shaped_line_index` keyed by hash, value = row index with valid shaped lines
3. Replace the linear scan in `find_matching_previous_row_ops_index` with a hash lookup

**Expected Gain:** Faster cache lookup for terminals with many repeated rows; eliminates O(n) scan

### 1.4 Thread-Local Buffer for `paint_damage_from_dirty_spans` (Quick Win)

**Current:** `paint_damage_from_dirty_spans` allocates a fresh `Vec<usize>` on every call.

**Solution:** Reuse a thread-local buffer.

```rust
thread_local! {
    static ROWS_BUF: RefCell<Vec<usize>> = RefCell::new(Vec::with_capacity(128));
}

fn paint_damage_from_dirty_spans_optimized(
    spans: &[TerminalDirtySpan],
    row_count: usize,
) -> TerminalGridPaintDamage {
    ROWS_BUF.with(|buf| {
        let mut rows = buf.borrow_mut();
        rows.clear();
        // ... populate rows ...
        TerminalGridPaintDamage::Rows(Arc::from(rows.as_slice()))
    })
}
```

**Expected Gain:** Eliminates one heap allocation per terminal wakeup event

---

## Phase 2: Memory Optimization

### 2.1 Scrollback Buffer Compression (High Impact, Requires alacritty_terminal Changes)

> **Note:** The scrollback buffer and `Cell` allocation are owned by `alacritty_terminal::Term`, not Termy code. This requires either patching `alacritty_terminal` as a path dependency or contributing upstream.

**Problem:** Default 2000 lines × terminal width × `Cell` size = significant RAM. Many scrollback lines are blank or repetitive.

**Solution (if patching alacritty_terminal):** Implement line-level compression at the `alacritty_terminal` grid layer.

```rust
// Would live in alacritty_terminal's grid module
enum LineContent {
    Blank(Color),                      // Entirely blank line (very common)
    RunLengthEncoded(Vec<CellRun>),    // Repetitive content
    Dense(Vec<Cell>),                  // Normal content
}
```

**Strategy:**
1. Detect uniform/blank lines on scroll-out (very common in scrollback)
2. Use run-length encoding for repetitive content
3. Keep active viewport always uncompressed

**Expected Gain:** 40-60% memory reduction for typical usage

**Alternative (no upstream changes):** Reduce default scrollback from 2000 to a lower configurable default and expose it prominently in settings.

### 2.2 Cell Color Resolution Cache (Quick Win)

**Problem:** `cell_fg_color` and `row_background_fill` are called for every cell every frame. Color comparisons involve floating-point `Hsla` fields.

**Solution:** Cache resolved `Hsla` → `gpui::Rgba` conversions.

```rust
pub struct ColorCache {
    cache: LruCache<[u32; 4], gpui::Rgba>, // Hsla bits -> Rgba
}
```

**Implementation:**
1. Add `ColorCache` to `TerminalGridPaintCache`
2. Wrap `cell_fg_color` resolution through the cache
3. Clear cache on style key change (theme/font change)

**Expected Gain:** Reduces floating-point work for terminals with many unique colors

### 2.3 String Interning for Repeated Content (Low Impact)

**Problem:** Tab titles, paths, and repeated text allocate duplicate `String`s.

**Solution:** Intern frequently occurring strings.

```rust
pub struct StringInterner {
    strings: HashMap<Arc<str>, Weak<str>>,
}

impl StringInterner {
    pub fn intern(&mut self, s: &str) -> Arc<str> {
        // Return existing Arc if present, insert otherwise
    }
}
```

---

## Phase 3: Input Latency Reduction

### 3.1 VSync-Aware Frame Scheduling (Medium Impact)

**Problem:** Inconsistent frame timing causes jitter when terminal output and input arrive mid-frame.

**Note:** GPUI owns the event loop and vsync. This must work within GPUI's `cx.request_animation_frame()` / `cx.notify()` model, not replace it.

**Solution:** Coalesce rapid `Wakeup` events within a single animation frame rather than triggering a repaint per event.

```rust
// In pane_terminal.rs — batch wakeup events within one frame boundary
pub struct WakeupCoalescer {
    pending: bool,
    last_frame: Instant,
}

impl WakeupCoalescer {
    pub fn record_wakeup(&mut self, cx: &mut Context<TerminalPane>) {
        if !self.pending {
            self.pending = true;
            cx.request_animation_frame(); // single repaint request
        }
    }
}
```

**Expected Gain:** Reduces redundant repaints when PTY output arrives in bursts

### 3.2 Predictive Cursor Rendering (Low Impact)

**Problem:** Cursor blink requires a repaint even when nothing else changed.

**Solution:** Pre-render both cursor states (visible/hidden) into cached quads so blink only swaps a flag, not a full paint pass.

```rust
pub struct CursorBlinkCache {
    visible_quad: Option<CachedQuad>,
    hidden: bool,
}
```

---

## Phase 4: Startup Time Optimization

### 4.1 Lazy Runtime Initialization (High Impact)

**Current:** Tmux verification and config loading block the initial window paint.

**Solution:** Show window immediately, initialize terminal runtime in background using GPUI's async model.

```rust
// In the terminal view init
impl TerminalPane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, config: AppConfig) -> Self {
        // Render placeholder immediately
        cx.spawn(|this, mut cx| async move {
            let runtime = init_runtime(&config).await;
            this.update(&mut cx, |this, cx| {
                this.attach_runtime(runtime, cx);
            }).ok();
        }).detach();

        Self { runtime: None, /* ... */ }
    }
}
```

**Expected Gain:** Window appears before tmux/shell startup completes; perceived startup time drops significantly

### 4.2 Parallel Subsystem Initialization (Medium Impact)

**Problem:** Font system, theme store, and config parsing initialize serially.

**Solution:** Use `std::thread::scope` to parallelize independent init work.

```rust
pub fn parallel_init() -> InitResult {
    std::thread::scope(|s| {
        let fonts = s.spawn(|| init_font_system());
        let themes = s.spawn(|| init_theme_store());
        let config = s.spawn(|| load_and_parse_config());
        InitResult {
            fonts: fonts.join().unwrap(),
            themes: themes.join().unwrap(),
            config: config.join().unwrap(),
        }
    })
}
```

### 4.3 Config Caching with Binary Format (Medium Impact)

**Problem:** JSON/TOML config parsing on every startup adds latency.

**Solution:** Cache parsed config in a binary format (e.g. `bincode`/`postcard`), invalidated by file mtime.

```rust
pub struct ConfigCache {
    source_path: PathBuf,
    cache_path: PathBuf,
}

impl ConfigCache {
    pub fn load(&self) -> AppConfig {
        // Check binary cache mtime vs source mtime
        // Fall back to full parse if stale or missing
        // Write binary cache on successful parse
    }
}
```

---

## Phase 5: Tmux & Runtime Optimization

### 5.1 Tmux Notification Coalescing — Already Implemented

`crates/terminal_ui/src/tmux/control/coalescer.rs` implements `NotificationCoalescer` with:
- Deduplication of `NeedsRefresh` events (only one queued at a time)
- Per-pane output merging for adjacent bursts (appends to tail if same pane ID)
- 512 KB output byte cap with oldest-chunk eviction and backpressure warnings
- Collapse-to-refresh on notification channel overflow

**Remaining opportunity:** Tune the 512 KB cap based on profiling. Consider per-pane caps for fairness when many panes are active.

### 5.2 Pane Rendering Prioritization (Medium Impact)

**Problem:** All panes are rendered equally, even fully occluded ones.

**Solution:** Track pane visibility and skip rendering for occluded panes.

```rust
pub struct PaneVisibility {
    visible: bool,
    occlusion_fraction: f32, // 0.0 = fully occluded, 1.0 = fully visible
    last_render: Instant,
}

// Skip TerminalGrid::paint for fully occluded panes
// Reduce damage rebuild rate for partially occluded panes
```

### 5.3 PTY Read Buffering (Medium Impact)

**Problem:** Frequent small reads from the PTY cause many syscalls.

**Solution:** Adaptive read buffer sizing.

```rust
pub struct PtyReader {
    buffer: Vec<u8>,
    target_read_size: usize, // Adaptive: 4 KB – 64 KB
}

impl PtyReader {
    pub fn read(&mut self, pty: &mut Pty) -> io::Result<&[u8]> {
        // Read as much as available in a single syscall
        // Grow buffer if consistently filling to capacity
    }
}
```

---

## Phase 6: Profiling & Monitoring Infrastructure

### 6.1 Frame Time Profiler — Extend Existing Metrics (High Priority)

**Current:** `render_metrics.rs` tracks five atomic counters:
- `grid_paint_count`
- `shape_line_calls`
- `shaped_line_cache_hits` / `shaped_line_cache_misses`
- `runtime_wakeup_count`

**Extend with span timing:**

```rust
// Add to render_metrics.rs
pub struct FrameProfiler {
    spans: Vec<TimedSpan>,
}

#[derive(Clone, Copy)]
pub enum SpanName {
    TmuxEventProcessing,
    GridDamageCompute,
    RowOpsRebuild,
    TextShaping,
    GpuSubmission,
}

// Usage
profiler.begin(SpanName::TextShaping);
let shaped = shape_line(text);
profiler.end(SpanName::TextShaping);
```

**Output:**
- Real-time overlay (debug builds) showing per-span ms
- Chrome trace format export (`chrome://tracing`)
- Automated regression detection in benchmark compare

### 6.2 Memory Tracker (Medium Priority)

```rust
pub struct MemoryTracker {
    categories: EnumMap<AllocCategory, CategoryStats>,
}

pub enum AllocCategory {
    GridCells,         // CachedRowPaintOps allocations
    ShapedLines,       // ShapedLine cache memory
    TmuxState,         // Notification coalescer + snapshot state
    ConfigData,        // Parsed config structs
}
```

### 6.3 Continuous Benchmarking — Already Implemented

`crates/xtask/src/benchmark.rs` provides a full benchmark driver and compare tool.

**Remaining gaps to fill:**
1. Add more scenarios to the existing `Scenario` enum:
   - `ScrollingOutput` — `yes | head -n 100000`
   - `RapidCursorMovement` — cursor movement stress
   - `SearchHeavyContent` — regex search in large buffer
   - `TmuxPaneResizing` — rapid pane splits/resizes
2. Add CI step to run `benchmark-compare` on every PR and fail on >5% regression
3. Export benchmark results as structured JSON for trend tracking

---

## Phase 7: Architecture Improvements

### 7.1 Copy-on-Write Grid Snapshots (Medium Impact)

**Current:** Grid data is cloned for rendering on each frame.

**Solution:** Share grid data with copy-on-write semantics.

```rust
pub struct GridSnapshot {
    data: Arc<GridData>,
    changes: Vec<GridChange>, // Delta since last snapshot
}

impl GridSnapshot {
    pub fn apply(&mut self, change: GridChange) {
        // Arc::make_mut clones only when shared
        Arc::make_mut(&mut self.data).apply(change);
    }
}
```

### 7.2 Render Thread Separation (High Impact, High Risk, High Effort)

**Goal:** Move all GPU command submission to a dedicated render thread so input processing is never blocked by GPU work.

> **Note:** This requires GPUI to support off-thread rendering or a custom Metal/wgpu command encoder. Verify GPUI's threading model before starting — this may require forking GPUI.

```rust
pub struct RenderThread {
    thread: JoinHandle<()>,
    command_queue: Channel<RenderCommand>,
    result_queue: Channel<RenderResult>,
}
// Main thread builds render description
// Render thread executes GPU commands
// Double-buffered swap chains
```

**Benefits:**
- Input processing never blocked by GPU
- Smoother frame times
- Better multi-core utilization

### 7.3 Plugin-Based Rendering (Low Priority, Future)

**Goal:** Allow alternative renderers (WebGPU, software fallback).

```rust
pub trait TerminalRenderer {
    fn render(&mut self, frame: &RenderFrame) -> RenderResult;
    fn resize(&mut self, size: TerminalSize);
    fn set_damage(&mut self, damage: TerminalGridPaintDamage);
}
```

---

## Implementation Priority Matrix

| Phase | Task | Impact | Effort | Priority | Status |
|-------|------|--------|--------|----------|--------|
| 1.4 | Thread-local damage buffer | Low | Low | P0 | Not started |
| 2.2 | Cell color resolution cache | Medium | Low | P0 | Not started |
| 1.2 | Cursor blink as overlay | High | Low | P0 | Done |
| 4.1 | Lazy runtime init | High | Medium | P0 | Not started |
| 6.1 | Frame time profiler spans | High | Medium | P0 | Not started |
| 1.1 | Cell-level damage tracking | High | Medium | P0 | Done |
| 6.3 | More benchmark scenarios + CI | High | Medium | P0 | Partial (driver exists) |
| 1.3 | Shaped-line hash index | Medium | Medium | P1 | Not started |
| 4.2 | Parallel init | Medium | Medium | P1 | Not started |
| 5.2 | Pane visibility culling | Medium | Medium | P1 | Not started |
| 3.1 | VSync-aware wakeup coalescing | Medium | Low | P1 | Not started |
| 4.3 | Config binary cache | Medium | Low | P2 | Not started |
| 5.3 | PTY read buffering | Medium | Low | P2 | Not started |
| 2.1 | Scrollback compression | High | Very High | P2 | Requires alacritty_terminal patch |
| 7.1 | CoW grid snapshots | Medium | High | P2 | Not started |
| 3.2 | Predictive cursor rendering | Low | Medium | P3 | Not started |
| 2.3 | String interning | Low | Low | P3 | Not started |
| 7.2 | Render thread separation | High | Very High | P3 | Requires GPUI investigation |
| 7.3 | Plugin rendering | Low | Very High | P3 | Future |
| 5.1 | Tmux event batching | — | — | Done | `coalescer.rs` |

---

## Already Implemented (Do Not Re-implement)

| Feature | Location |
|---------|----------|
| Tmux notification coalescing (per-pane merge, backpressure, 512 KB cap) | `tmux/control/coalescer.rs` |
| Text run batching by `(bold, fg, underline)` per row | `grid.rs` — `TextBatch` |
| Background span batching by color per row | `grid.rs` — `BackgroundSpan` |
| Unicode block elements as pixel-snapped quads | `grid.rs` — `block_element_geometry` |
| Per-row `ShapedLine` cache with cross-row reuse | `grid.rs` — `CachedRowPaintOps.shaped_lines` |
| Benchmark driver, compare tool, scenario runner | `crates/xtask/src/benchmark.rs` |
| Atomic render metrics (5 counters) | `render_metrics.rs` |

---

## Success Metrics

After implementing this plan:

1. **Frame Time:** p99 frame time < 16.67ms (60 FPS) during `cat` of 100MB file
2. **Memory:** Base footprint < 100MB with default 2000-line scrollback
3. **Latency:** Key-to-screen latency < 8ms measured with photodiode
4. **Startup:** Cold start to interactive < 200ms on M1 MacBook Pro
5. **Efficiency:** Zero CPU usage when idle (no busy-waiting)
6. **Regression gate:** No PR merges with >5% benchmark regression

---

## Appendix: Benchmark Commands

```bash
# Frame time stress test
yes | head -n 1000000 | base64

# Scrollback memory test
cat /dev/urandom | base64 | head -c 50M

# Input latency test
# Use typometer or similar tool

# Startup time
hyperfine 'termy -e exit'

# Run benchmark driver
cargo run -p xtask -- benchmark-driver --scenario <name> --duration-secs 13

# Compare two builds
cargo run -p xtask -- benchmark-compare --baseline <spec> --candidate <spec>
```

---

*Last updated: 2026-03-23* (1.1, 1.2 completed)
*Owner: Performance Working Group*
*Review cycle: Monthly*
