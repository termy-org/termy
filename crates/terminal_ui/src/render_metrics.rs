#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalUiRenderMetricsSnapshot {
    pub grid_paint_count: u64,
    pub shape_line_calls: u64,
    pub shaped_line_cache_hits: u64,
    pub shaped_line_cache_misses: u64,
    pub runtime_wakeup_count: u64,
    pub span_damage_compute_us: u64,
    pub span_row_ops_rebuild_us: u64,
    pub span_text_shaping_us: u64,
    pub span_grid_paint_us: u64,
}

impl TerminalUiRenderMetricsSnapshot {
    pub fn saturating_sub(self, previous: Self) -> Self {
        Self {
            grid_paint_count: self
                .grid_paint_count
                .saturating_sub(previous.grid_paint_count),
            shape_line_calls: self
                .shape_line_calls
                .saturating_sub(previous.shape_line_calls),
            shaped_line_cache_hits: self
                .shaped_line_cache_hits
                .saturating_sub(previous.shaped_line_cache_hits),
            shaped_line_cache_misses: self
                .shaped_line_cache_misses
                .saturating_sub(previous.shaped_line_cache_misses),
            runtime_wakeup_count: self
                .runtime_wakeup_count
                .saturating_sub(previous.runtime_wakeup_count),
            span_damage_compute_us: self
                .span_damage_compute_us
                .saturating_sub(previous.span_damage_compute_us),
            span_row_ops_rebuild_us: self
                .span_row_ops_rebuild_us
                .saturating_sub(previous.span_row_ops_rebuild_us),
            span_text_shaping_us: self
                .span_text_shaping_us
                .saturating_sub(previous.span_text_shaping_us),
            span_grid_paint_us: self
                .span_grid_paint_us
                .saturating_sub(previous.span_grid_paint_us),
        }
    }
}

// Keep render metrics active in release benchmarks and tests. The counters stay
// dormant unless the app reads them.
static GRID_PAINT_COUNT: AtomicU64 = AtomicU64::new(0);
static SHAPE_LINE_CALLS: AtomicU64 = AtomicU64::new(0);
static SHAPED_LINE_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static SHAPED_LINE_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static RUNTIME_WAKEUP_COUNT: AtomicU64 = AtomicU64::new(0);
static SPAN_DAMAGE_COMPUTE_US: AtomicU64 = AtomicU64::new(0);
static SPAN_ROW_OPS_REBUILD_US: AtomicU64 = AtomicU64::new(0);
static SPAN_TEXT_SHAPING_US: AtomicU64 = AtomicU64::new(0);
static SPAN_GRID_PAINT_US: AtomicU64 = AtomicU64::new(0);

fn increment_counter(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}

pub(crate) fn increment_grid_paint_count() {
    increment_counter(&GRID_PAINT_COUNT);
}

pub(crate) fn increment_shape_line_calls() {
    increment_counter(&SHAPE_LINE_CALLS);
}

pub(crate) fn increment_shaped_line_cache_hit() {
    increment_counter(&SHAPED_LINE_CACHE_HITS);
}

pub(crate) fn increment_shaped_line_cache_miss() {
    increment_counter(&SHAPED_LINE_CACHE_MISSES);
}

pub(crate) fn increment_runtime_wakeup_count() {
    increment_counter(&RUNTIME_WAKEUP_COUNT);
}

fn add_to_counter(counter: &AtomicU64, micros: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(micros))
    });
}

pub fn add_span_damage_compute_us(micros: u64) {
    add_to_counter(&SPAN_DAMAGE_COMPUTE_US, micros);
}

pub(crate) fn add_span_row_ops_rebuild_us(micros: u64) {
    add_to_counter(&SPAN_ROW_OPS_REBUILD_US, micros);
}

pub(crate) fn add_span_text_shaping_us(micros: u64) {
    add_to_counter(&SPAN_TEXT_SHAPING_US, micros);
}

pub(crate) fn add_span_grid_paint_us(micros: u64) {
    add_to_counter(&SPAN_GRID_PAINT_US, micros);
}

pub fn terminal_ui_render_metrics_snapshot() -> TerminalUiRenderMetricsSnapshot {
    TerminalUiRenderMetricsSnapshot {
        grid_paint_count: GRID_PAINT_COUNT.load(Ordering::Relaxed),
        shape_line_calls: SHAPE_LINE_CALLS.load(Ordering::Relaxed),
        shaped_line_cache_hits: SHAPED_LINE_CACHE_HITS.load(Ordering::Relaxed),
        shaped_line_cache_misses: SHAPED_LINE_CACHE_MISSES.load(Ordering::Relaxed),
        runtime_wakeup_count: RUNTIME_WAKEUP_COUNT.load(Ordering::Relaxed),
        span_damage_compute_us: SPAN_DAMAGE_COMPUTE_US.load(Ordering::Relaxed),
        span_row_ops_rebuild_us: SPAN_ROW_OPS_REBUILD_US.load(Ordering::Relaxed),
        span_text_shaping_us: SPAN_TEXT_SHAPING_US.load(Ordering::Relaxed),
        span_grid_paint_us: SPAN_GRID_PAINT_US.load(Ordering::Relaxed),
    }
}

pub fn terminal_ui_render_metrics_reset() {
    GRID_PAINT_COUNT.store(0, Ordering::Relaxed);
    SHAPE_LINE_CALLS.store(0, Ordering::Relaxed);
    SHAPED_LINE_CACHE_HITS.store(0, Ordering::Relaxed);
    SHAPED_LINE_CACHE_MISSES.store(0, Ordering::Relaxed);
    RUNTIME_WAKEUP_COUNT.store(0, Ordering::Relaxed);
    SPAN_DAMAGE_COMPUTE_US.store(0, Ordering::Relaxed);
    SPAN_ROW_OPS_REBUILD_US.store(0, Ordering::Relaxed);
    SPAN_TEXT_SHAPING_US.store(0, Ordering::Relaxed);
    SPAN_GRID_PAINT_US.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_METRICS_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn snapshot_is_zero_after_reset() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        assert_eq!(
            terminal_ui_render_metrics_snapshot(),
            TerminalUiRenderMetricsSnapshot::default()
        );
    }

    #[test]
    fn increment_grid_paint_updates_snapshot() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        increment_grid_paint_count();
        let snapshot = terminal_ui_render_metrics_snapshot();
        assert_eq!(snapshot.grid_paint_count, 1);
        assert_eq!(snapshot.shape_line_calls, 0);
        assert_eq!(snapshot.shaped_line_cache_hits, 0);
        assert_eq!(snapshot.shaped_line_cache_misses, 0);
        assert_eq!(snapshot.runtime_wakeup_count, 0);
    }

    #[test]
    fn increment_shape_line_updates_snapshot() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        increment_shape_line_calls();
        increment_shape_line_calls();
        let snapshot = terminal_ui_render_metrics_snapshot();
        assert_eq!(snapshot.grid_paint_count, 0);
        assert_eq!(snapshot.shape_line_calls, 2);
        assert_eq!(snapshot.shaped_line_cache_hits, 0);
        assert_eq!(snapshot.shaped_line_cache_misses, 0);
        assert_eq!(snapshot.runtime_wakeup_count, 0);
    }

    #[test]
    fn increment_shaped_line_cache_updates_snapshot() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        increment_shaped_line_cache_hit();
        increment_shaped_line_cache_hit();
        increment_shaped_line_cache_miss();
        let snapshot = terminal_ui_render_metrics_snapshot();
        assert_eq!(snapshot.grid_paint_count, 0);
        assert_eq!(snapshot.shape_line_calls, 0);
        assert_eq!(snapshot.shaped_line_cache_hits, 2);
        assert_eq!(snapshot.shaped_line_cache_misses, 1);
        assert_eq!(snapshot.runtime_wakeup_count, 0);
    }

    #[test]
    fn increment_runtime_wakeup_updates_snapshot() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        increment_runtime_wakeup_count();
        let snapshot = terminal_ui_render_metrics_snapshot();
        assert_eq!(snapshot.grid_paint_count, 0);
        assert_eq!(snapshot.shape_line_calls, 0);
        assert_eq!(snapshot.shaped_line_cache_hits, 0);
        assert_eq!(snapshot.shaped_line_cache_misses, 0);
        assert_eq!(snapshot.runtime_wakeup_count, 1);
    }

    #[test]
    fn reset_clears_counters_after_increments() {
        let _guard = TEST_METRICS_MUTEX.lock().unwrap();
        terminal_ui_render_metrics_reset();
        increment_grid_paint_count();
        increment_shape_line_calls();
        increment_shaped_line_cache_hit();
        increment_shaped_line_cache_miss();
        increment_runtime_wakeup_count();
        terminal_ui_render_metrics_reset();
        assert_eq!(
            terminal_ui_render_metrics_snapshot(),
            TerminalUiRenderMetricsSnapshot::default()
        );
    }
}
