mod grid;
mod keyboard;
mod links;
mod locale;
mod monotonic_time;
mod mouse_protocol;
mod pane_terminal;
mod path_env;
mod protocol;
mod render_metrics;
mod runtime;
mod tmux;

// Intentionally re-exported for the app renderer adapter boundary. These types are the
// cross-crate contract for row-level paint-cache invalidation between `termy` and this crate.
pub use grid::{
    CellRenderInfo, TerminalCursorStyle, TerminalGrid, TerminalGridPaintCacheHandle,
    TerminalGridPaintDamage, TerminalGridRow, TerminalGridRows,
};
pub use keyboard::{TerminalKeyEventKind, TerminalKeyboardMode, keystroke_to_input};
pub use links::{DetectedLink, classify_link_token, find_link_in_line};
pub use monotonic_time::terminal_ui_monotonic_now_ns;
pub use mouse_protocol::{
    TerminalMouseButton, TerminalMouseEventKind, TerminalMouseMode, TerminalMouseModifiers,
    TerminalMousePosition, encode_mouse_report,
};
pub use pane_terminal::PaneTerminal;
pub use protocol::{TerminalClipboardTarget, TerminalQueryColors, TerminalReplyHost};
pub use render_metrics::{
    TerminalUiRenderMetricsSnapshot, terminal_ui_render_metrics_reset,
    terminal_ui_render_metrics_snapshot,
};
pub use runtime::{
    normalize_working_directory_candidate, resolve_launch_working_directory,
    resolve_working_directory_path,
    TabTitleShellIntegration, Terminal, TerminalCursorState, TerminalDamageSnapshot,
    TerminalDirtySpan, TerminalEvent, TerminalOptions, TerminalRuntimeConfig, TerminalSize,
    WorkingDirFallback,
};
pub use tmux::{
    TmuxClient, TmuxLaunchTarget, TmuxNotification, TmuxPaneState, TmuxRuntimeConfig,
    TmuxSessionSummary, TmuxShutdownMode, TmuxSnapshot, TmuxSocketTarget, TmuxWindowState,
};
