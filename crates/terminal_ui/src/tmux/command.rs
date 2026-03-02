use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const SEND_INPUT_CHUNKED_HEX_BYTES: usize = 256;
pub(crate) const SEND_INPUT_BULK_THRESHOLD_BYTES: usize = 2048;
pub(crate) const SEND_INPUT_BULK_HEX_BYTES: usize = 2048;
const CONTROL_COMPLETION_TOKEN_PREFIX: &str = "__termy_cmd_done_";

static CONTROL_COMPLETION_TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SendInputMode {
    ChunkedHex,
    Bulk,
}

pub(crate) fn tmux_command_line(args: &[&str]) -> String {
    args.iter()
        .map(|arg| quote_tmux_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn next_control_completion_token() -> String {
    let id = CONTROL_COMPLETION_TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{CONTROL_COMPLETION_TOKEN_PREFIX}{id}")
}

pub(crate) fn command_with_completion_token(command: &str, completion_token: &str) -> String {
    format!(
        "{command} ; display-message -p {}",
        quote_tmux_arg(completion_token)
    )
}

pub(crate) fn split_control_completion_token(
    output: &str,
    completion_token: &str,
) -> Option<String> {
    if output == completion_token {
        return Some(String::new());
    }

    let token_suffix = format!("\n{completion_token}");
    output
        .strip_suffix(token_suffix.as_str())
        .map(ToOwned::to_owned)
}

pub(crate) fn choose_send_input_mode(bytes_len: usize) -> (SendInputMode, usize) {
    if bytes_len >= SEND_INPUT_BULK_THRESHOLD_BYTES {
        return (
            SendInputMode::Bulk,
            bytes_len.div_ceil(SEND_INPUT_BULK_HEX_BYTES),
        );
    }

    (
        SendInputMode::ChunkedHex,
        bytes_len.div_ceil(SEND_INPUT_CHUNKED_HEX_BYTES),
    )
}

pub(crate) fn send_keys_hex_command(pane_id: &str, chunk: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut command = String::with_capacity(18 + pane_id.len() + (chunk.len() * 3));
    command.push_str("send-keys -t ");
    command.push_str(pane_id);
    command.push_str(" -H");
    for byte in chunk {
        write!(&mut command, " {byte:02x}").expect("writing hex bytes into String cannot fail");
    }
    command
}

pub(crate) fn quote_tmux_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_./:@%+#,=".contains(&byte))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_input_uses_chunked_hex_path_for_small_payloads() {
        let (mode, chunks) = choose_send_input_mode(1024);
        assert_eq!(mode, SendInputMode::ChunkedHex);
        assert_eq!(chunks, 4);
    }

    #[test]
    fn send_input_uses_bulk_path_for_large_payloads() {
        let (mode, chunks) = choose_send_input_mode(8192);
        assert_eq!(mode, SendInputMode::Bulk);
        assert_eq!(chunks, 4);
        assert!(chunks < 64);
    }

    #[test]
    fn send_input_switches_to_bulk_at_threshold() {
        let (small_mode, small_chunks) = choose_send_input_mode(2047);
        assert_eq!(small_mode, SendInputMode::ChunkedHex);
        assert_eq!(small_chunks, 8);

        let (bulk_mode, bulk_chunks) = choose_send_input_mode(2048);
        assert_eq!(bulk_mode, SendInputMode::Bulk);
        assert_eq!(bulk_chunks, 1);
    }

    #[test]
    fn quote_tmux_arg_single_quotes_embedded_quotes() {
        assert_eq!(
            quote_tmux_arg("pane name with spaces and 'quote'"),
            "'pane name with spaces and '\\''quote'\\'''"
        );
    }
}
