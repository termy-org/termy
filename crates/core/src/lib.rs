mod config;
mod frame;
mod keyboard;
mod links;
mod locale;
mod monotonic_time;
mod mouse_protocol;
mod osc_intercept;
mod path_env;
mod protocol;
mod render_metrics;
mod runtime;
mod shell_integration;

pub use config::{
    LoadedTermyConfig, TermyConfigError, load_config_from_contents, load_config_from_default_path,
    load_config_from_path, runtime_config_from_app_config,
    runtime_config_from_app_config_with_query_colors,
};
pub use frame::{TermyCell, TermyColor, TermyFrame};
pub use keyboard::{
    Keystroke, Modifiers, TerminalKeyEventKind, TerminalKeyboardMode, TermyKeystroke,
    TermyModifiers, keystroke_to_input,
};
pub use links::{DetectedLink, classify_link_token, find_link_in_line};
pub use monotonic_time::terminal_ui_monotonic_now_ns;
pub use mouse_protocol::{
    TerminalMouseButton, TerminalMouseEventKind, TerminalMouseMode, TerminalMouseModifiers,
    TerminalMousePosition, encode_mouse_report,
};
pub use osc_intercept::{OscEvent, OscInterceptor};
pub use protocol::{TerminalClipboardTarget, TerminalQueryColors, TerminalReplyHost};
pub use render_metrics::{
    TerminalUiRenderMetricsSnapshot, add_span_damage_compute_us, add_span_grid_paint_us,
    add_span_row_ops_rebuild_us, add_span_text_shaping_us, increment_grid_paint_count,
    increment_shape_line_calls, increment_shaped_line_cache_hit, increment_shaped_line_cache_miss,
    terminal_ui_render_metrics_reset, terminal_ui_render_metrics_snapshot,
};
pub use runtime::{
    TabTitleShellIntegration, Terminal, TerminalCursorState, TerminalCursorStyle,
    TerminalDamageSnapshot, TerminalDirtySpan, TerminalEvent, TerminalOptions,
    TerminalRuntimeConfig, TerminalSize, WorkingDirFallback, normalize_working_directory_candidate,
    resolve_launch_working_directory, resolve_working_directory_path,
};
pub use shell_integration::{CommandLifecycle, CommandPhase, ProgressState};
pub use termy_config_core::{
    AppConfig, ConfigDiagnostic, ConfigDiagnosticKind, ConfigParseReport,
    CursorStyle as AppConfigCursorStyle, config_path,
};
