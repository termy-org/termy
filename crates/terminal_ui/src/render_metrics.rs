pub use termy_core::{
    TerminalUiRenderMetricsSnapshot, add_span_damage_compute_us, add_span_grid_paint_us,
    add_span_row_ops_rebuild_us, add_span_text_shaping_us, increment_grid_paint_count,
    increment_shape_line_calls, increment_shaped_line_cache_hit, increment_shaped_line_cache_miss,
    terminal_ui_render_metrics_reset, terminal_ui_render_metrics_snapshot,
};
