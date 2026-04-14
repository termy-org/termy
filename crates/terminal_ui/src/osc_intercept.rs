//! OSC escape sequence interceptor for sequences not handled by alacritty_terminal.
//!
//! This module pre-parses the PTY output stream to extract OSC sequences that
//! alacritty_terminal does not expose as events:
//! - OSC 7: Working directory (file:// URL)
//! - OSC 9: iTerm2-style notification
//! - OSC 9;4: Progress indicator (ConEmu/Windows Terminal)
//! - OSC 133: Shell integration (prompt/command lifecycle)
//! - OSC 777: Desktop notification with title

use crate::shell_integration::ProgressState;

/// Events extracted from custom OSC sequences
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OscEvent {
    /// OSC 7 - Working directory change
    /// Format: ESC ] 7 ; file://hostname/path ST
    WorkingDirectory(String),

    /// OSC 9 - Simple notification (iTerm2 style)
    /// Format: ESC ] 9 ; message ST
    Notify(String),

    /// OSC 9;4 - Progress indicator (ConEmu/Windows Terminal)
    /// Format: ESC ] 9 ; 4 ; state ; progress ST
    Progress(ProgressState),

    /// OSC 133;A - Prompt start
    ShellPromptStart,

    /// OSC 133;B - Command input start
    ShellCommandStart,

    /// OSC 133;C - Command executing
    ShellCommandExecuting,

    /// OSC 133;D - Command finished with optional exit code
    ShellCommandFinished(Option<i32>),

    /// OSC 777 - Desktop notification with title
    /// Format: ESC ] 777 ; notify ; title ; body ST
    Notification { title: String, body: String },
}

/// State machine for parsing OSC sequences from a byte stream.
///
/// The interceptor processes bytes and extracts custom OSC sequences,
/// passing through all other data unchanged.
#[derive(Debug, Default)]
pub struct OscInterceptor {
    /// Buffer for accumulating OSC payload
    buffer: Vec<u8>,
    /// Current parsing state
    state: ParseState,
}

/// Maximum size for the OSC buffer to prevent unbounded memory growth from malformed input.
const MAX_OSC_BUFFER: usize = 64 * 1024; // 64 KB

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    /// Normal passthrough mode
    #[default]
    Ground,
    /// Saw ESC (0x1B)
    Escape,
    /// Saw ESC ] (OSC start)
    OscStart,
    /// Accumulating OSC payload
    OscPayload,
    /// Saw ESC within OSC (possible ST)
    OscEscape,
}

impl OscInterceptor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process input bytes, extracting custom OSC sequences.
    ///
    /// Returns a tuple of:
    /// - Filtered bytes (with extracted OSC sequences removed)
    /// - Vector of extracted OSC events
    pub fn process(&mut self, input: &[u8]) -> (Vec<u8>, Vec<OscEvent>) {
        let mut output = Vec::with_capacity(input.len());
        let mut events = Vec::new();

        for &byte in input {
            match self.state {
                ParseState::Ground => {
                    if byte == 0x1B {
                        // ESC
                        self.state = ParseState::Escape;
                    } else {
                        output.push(byte);
                    }
                }

                ParseState::Escape => {
                    if byte == b']' {
                        // ESC ] = OSC start
                        self.state = ParseState::OscStart;
                        self.buffer.clear();
                    } else {
                        // Not an OSC, pass through ESC and this byte
                        output.push(0x1B);
                        output.push(byte);
                        self.state = ParseState::Ground;
                    }
                }

                ParseState::OscStart => {
                    // First byte after ESC ] determines if we care about this OSC
                    self.buffer.push(byte);
                    self.state = ParseState::OscPayload;
                }

                ParseState::OscPayload => {
                    if byte == 0x07 {
                        // BEL = OSC terminator
                        if let Some(event) = self.parse_osc_payload() {
                            events.push(event);
                        } else {
                            // Not a custom OSC, pass it through
                            self.emit_osc_to_output(&mut output);
                        }
                        self.buffer.clear();
                        self.state = ParseState::Ground;
                    } else if byte == 0x1B {
                        // ESC within OSC - might be ST (ESC \)
                        self.state = ParseState::OscEscape;
                    } else if self.buffer.len() < MAX_OSC_BUFFER {
                        self.buffer.push(byte);
                    } else {
                        // Buffer overflow - emit partial OSC and reset
                        self.emit_osc_to_output(&mut output);
                        self.buffer.clear();
                        self.state = ParseState::Ground;
                    }
                }

                ParseState::OscEscape => {
                    if byte == b'\\' {
                        // ESC \ = ST (String Terminator)
                        if let Some(event) = self.parse_osc_payload() {
                            events.push(event);
                        } else {
                            // Not a custom OSC, pass it through
                            self.emit_osc_to_output(&mut output);
                        }
                        self.buffer.clear();
                        self.state = ParseState::Ground;
                    } else if self.buffer.len() + 2 <= MAX_OSC_BUFFER {
                        // Not ST, add ESC to buffer and continue
                        self.buffer.push(0x1B);
                        self.buffer.push(byte);
                        self.state = ParseState::OscPayload;
                    } else {
                        // Buffer overflow - emit partial OSC and reset
                        self.emit_osc_to_output(&mut output);
                        self.buffer.clear();
                        self.state = ParseState::Ground;
                    }
                }
            }
        }

        (output, events)
    }

    /// Emit the current OSC buffer as passthrough to output
    fn emit_osc_to_output(&self, output: &mut Vec<u8>) {
        output.push(0x1B);
        output.push(b']');
        output.extend_from_slice(&self.buffer);
        output.push(0x07);
    }

    /// Try to parse the OSC payload as a custom sequence we handle
    fn parse_osc_payload(&self) -> Option<OscEvent> {
        let payload = std::str::from_utf8(&self.buffer).ok()?;

        // OSC 7 - Working directory
        if let Some(url) = payload.strip_prefix("7;") {
            return Some(OscEvent::WorkingDirectory(
                parse_file_url(url).unwrap_or_else(|| url.to_string()),
            ));
        }

        // OSC 9 - Notification or progress
        if let Some(rest) = payload.strip_prefix("9;") {
            // OSC 9;4;state;progress - Progress indicator
            if let Some(progress_part) = rest.strip_prefix("4;") {
                return parse_progress(progress_part);
            }
            // OSC 9;message - Simple notification
            return Some(OscEvent::Notify(rest.to_string()));
        }

        // OSC 133 - Shell integration
        if let Some(rest) = payload.strip_prefix("133;") {
            return parse_shell_integration(rest);
        }

        // OSC 777 - Desktop notification
        if let Some(rest) = payload.strip_prefix("777;") {
            return parse_notification_777(rest);
        }

        None
    }
}

/// Parse OSC 7 file:// URL to extract path
fn parse_file_url(url: &str) -> Option<String> {
    // Format: file://hostname/path or file:///path
    if let Some(rest) = url.strip_prefix("file://") {
        // Skip hostname (everything up to first / after hostname)
        if let Some(slash_pos) = rest.find('/') {
            return Some(rest[slash_pos..].to_string());
        }
    }
    None
}

/// Parse OSC 9;4 progress indicator
fn parse_progress(payload: &str) -> Option<OscEvent> {
    // Format: state;progress
    let mut parts = payload.split(';');
    let state: u8 = parts.next()?.parse().ok()?;
    let progress: u8 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some(OscEvent::Progress(ProgressState::from_osc(state, progress)))
}

/// Parse OSC 133 shell integration sequences
fn parse_shell_integration(payload: &str) -> Option<OscEvent> {
    // Format: A, B, C, or D;exit_code
    match payload.chars().next()? {
        'A' => Some(OscEvent::ShellPromptStart),
        'B' => Some(OscEvent::ShellCommandStart),
        'C' => Some(OscEvent::ShellCommandExecuting),
        'D' => {
            // D may be followed by ;exit_code
            let exit_code = payload
                .strip_prefix("D;")
                .and_then(|s| s.parse::<i32>().ok());
            Some(OscEvent::ShellCommandFinished(exit_code))
        }
        _ => None,
    }
}

/// Parse OSC 777 notification
fn parse_notification_777(payload: &str) -> Option<OscEvent> {
    // Format: notify;title;body
    if let Some(rest) = payload.strip_prefix("notify;") {
        let mut parts = rest.splitn(2, ';');
        let title = parts.next()?.to_string();
        let body = parts.next().unwrap_or("").to_string();
        return Some(OscEvent::Notification { title, body });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_str(interceptor: &mut OscInterceptor, s: &str) -> (String, Vec<OscEvent>) {
        let (bytes, events) = interceptor.process(s.as_bytes());
        (String::from_utf8_lossy(&bytes).into_owned(), events)
    }

    #[test]
    fn passthrough_normal_text() {
        let mut interceptor = OscInterceptor::new();
        let (output, events) = process_str(&mut interceptor, "hello world");
        assert_eq!(output, "hello world");
        assert!(events.is_empty());
    }

    #[test]
    fn passthrough_unknown_osc() {
        let mut interceptor = OscInterceptor::new();
        // OSC 0 (title) should be passed through
        let (output, events) = process_str(&mut interceptor, "\x1b]0;My Title\x07");
        assert_eq!(output, "\x1b]0;My Title\x07");
        assert!(events.is_empty());
    }

    #[test]
    fn parse_osc_7_working_directory() {
        let mut interceptor = OscInterceptor::new();
        let (output, events) =
            process_str(&mut interceptor, "\x1b]7;file://localhost/home/user\x07");
        assert!(output.is_empty());
        assert_eq!(
            events,
            vec![OscEvent::WorkingDirectory("/home/user".to_string())]
        );
    }

    #[test]
    fn parse_osc_9_notification() {
        let mut interceptor = OscInterceptor::new();
        let (output, events) = process_str(&mut interceptor, "\x1b]9;Build complete!\x07");
        assert!(output.is_empty());
        assert_eq!(
            events,
            vec![OscEvent::Notify("Build complete!".to_string())]
        );
    }

    #[test]
    fn parse_osc_9_4_progress() {
        let mut interceptor = OscInterceptor::new();

        // Progress 50%
        let (_, events) = process_str(&mut interceptor, "\x1b]9;4;1;50\x07");
        assert_eq!(
            events,
            vec![OscEvent::Progress(ProgressState::InProgress(50))]
        );

        // Indeterminate
        let (_, events) = process_str(&mut interceptor, "\x1b]9;4;3;0\x07");
        assert_eq!(
            events,
            vec![OscEvent::Progress(ProgressState::Indeterminate)]
        );

        // Clear
        let (_, events) = process_str(&mut interceptor, "\x1b]9;4;0;0\x07");
        assert_eq!(events, vec![OscEvent::Progress(ProgressState::Clear)]);

        // Error
        let (_, events) = process_str(&mut interceptor, "\x1b]9;4;2;75\x07");
        assert_eq!(events, vec![OscEvent::Progress(ProgressState::Error(75))]);
    }

    #[test]
    fn parse_osc_133_shell_integration() {
        let mut interceptor = OscInterceptor::new();

        let (_, events) = process_str(&mut interceptor, "\x1b]133;A\x07");
        assert_eq!(events, vec![OscEvent::ShellPromptStart]);

        let (_, events) = process_str(&mut interceptor, "\x1b]133;B\x07");
        assert_eq!(events, vec![OscEvent::ShellCommandStart]);

        let (_, events) = process_str(&mut interceptor, "\x1b]133;C\x07");
        assert_eq!(events, vec![OscEvent::ShellCommandExecuting]);

        let (_, events) = process_str(&mut interceptor, "\x1b]133;D;0\x07");
        assert_eq!(events, vec![OscEvent::ShellCommandFinished(Some(0))]);

        let (_, events) = process_str(&mut interceptor, "\x1b]133;D;1\x07");
        assert_eq!(events, vec![OscEvent::ShellCommandFinished(Some(1))]);

        // D without exit code
        let (_, events) = process_str(&mut interceptor, "\x1b]133;D\x07");
        assert_eq!(events, vec![OscEvent::ShellCommandFinished(None)]);
    }

    #[test]
    fn parse_osc_777_notification() {
        let mut interceptor = OscInterceptor::new();
        let (output, events) = process_str(
            &mut interceptor,
            "\x1b]777;notify;Build;Completed successfully\x07",
        );
        assert!(output.is_empty());
        assert_eq!(
            events,
            vec![OscEvent::Notification {
                title: "Build".to_string(),
                body: "Completed successfully".to_string()
            }]
        );
    }

    #[test]
    fn parse_osc_with_st_terminator() {
        let mut interceptor = OscInterceptor::new();
        // ST = ESC \ instead of BEL
        let (output, events) = process_str(&mut interceptor, "\x1b]9;Test message\x1b\\");
        assert!(output.is_empty());
        assert_eq!(events, vec![OscEvent::Notify("Test message".to_string())]);
    }

    #[test]
    fn multiple_sequences_in_stream() {
        let mut interceptor = OscInterceptor::new();
        let input = "prefix\x1b]133;A\x07middle\x1b]9;4;1;50\x07suffix";
        let (output, events) = process_str(&mut interceptor, input);
        assert_eq!(output, "prefixmiddlesuffix");
        assert_eq!(
            events,
            vec![
                OscEvent::ShellPromptStart,
                OscEvent::Progress(ProgressState::InProgress(50))
            ]
        );
    }

    #[test]
    fn interleaved_with_other_escapes() {
        let mut interceptor = OscInterceptor::new();
        // Mix of cursor movement (CSI) and our OSC
        let input = "\x1b[Hstart\x1b]133;C\x07\x1b[Jend";
        let (output, events) = process_str(&mut interceptor, input);
        // CSI sequences should pass through
        assert_eq!(output, "\x1b[Hstart\x1b[Jend");
        assert_eq!(events, vec![OscEvent::ShellCommandExecuting]);
    }

    #[test]
    fn incremental_processing() {
        let mut interceptor = OscInterceptor::new();

        // Send partial OSC
        let (output1, events1) = process_str(&mut interceptor, "\x1b]133;");
        assert!(output1.is_empty());
        assert!(events1.is_empty());

        // Complete the OSC
        let (output2, events2) = process_str(&mut interceptor, "A\x07rest");
        assert_eq!(output2, "rest");
        assert_eq!(events2, vec![OscEvent::ShellPromptStart]);
    }
}
