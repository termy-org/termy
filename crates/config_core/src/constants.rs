use crate::schema::ROOT_SETTING_ALL_KEYS;

pub(crate) const DEFAULT_TAB_TITLE_FALLBACK: &str = "Terminal";
pub(crate) const DEFAULT_TAB_TITLE_EXPLICIT_PREFIX: &str = "termy:tab:";
pub(crate) const DEFAULT_TAB_TITLE_PROMPT_FORMAT: &str = "{cwd}";
pub(crate) const DEFAULT_TAB_TITLE_COMMAND_FORMAT: &str = "{command}";
pub(crate) const DEFAULT_TERM: &str = "xterm-256color";
pub(crate) const DEFAULT_COLORTERM: &str = "truecolor";
pub(crate) const DEFAULT_TMUX_ENABLED: bool = false;
pub(crate) const DEFAULT_TMUX_BINARY: &str = "tmux";
pub(crate) const DEFAULT_TMUX_PERSISTENCE: bool = true;
pub(crate) const DEFAULT_TMUX_SHOW_ACTIVE_PANE_BORDER: bool = false;
pub(crate) const DEFAULT_MOUSE_SCROLL_MULTIPLIER: f32 = 3.0;
pub(crate) const DEFAULT_SCROLLBACK_HISTORY: usize = 2000;
pub(crate) const MAX_SCROLLBACK_HISTORY: usize = 100_000;
pub(crate) const DEFAULT_INACTIVE_TAB_SCROLLBACK: Option<usize> = None;
pub(crate) const DEFAULT_PANE_FOCUS_STRENGTH: f32 = 0.6;
pub(crate) const DEFAULT_TAB_SWITCH_MODIFIER_HINTS: bool = true;
pub(crate) const MAX_PANE_FOCUS_STRENGTH: f32 = 2.0;
pub(crate) const MIN_MOUSE_SCROLL_MULTIPLIER: f32 = 0.1;
pub(crate) const MAX_MOUSE_SCROLL_MULTIPLIER: f32 = 1_000.0;
pub(crate) const DEFAULT_CURSOR_BLINK: bool = true;
pub(crate) const DEFAULT_WARN_ON_QUIT: bool = false;
pub(crate) const DEFAULT_WARN_ON_QUIT_WITH_RUNNING_PROCESS: bool = true;

pub const VALID_ROOT_KEYS: &[&str] = ROOT_SETTING_ALL_KEYS;

pub const VALID_SECTIONS: &[&str] = &["colors", "tab_title"];

pub const SHELL_DECIDE_THEME_ID: &str = "shell-decide";
