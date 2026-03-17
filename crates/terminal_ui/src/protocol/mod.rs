// PTY reply handling lives here so future upstream Alacritty query events can be
// added in one place without growing runtime.rs into another protocol dump.
mod query_colors;
mod replies;

pub use query_colors::TerminalQueryColors;
pub use replies::{TerminalClipboardTarget, TerminalReplyHost};
pub(crate) use replies::reply_bytes_for_event;
