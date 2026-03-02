pub(crate) fn strip_control_line_wrappers(mut line: &[u8]) -> &[u8] {
    // tmux control mode may wrap protocol lines in DCS passthrough sequences
    // (for example: ESC P1000p ... ESC \\). Strip wrappers so parser matching
    // stays stable across tmux/terminal combinations.
    while let Some(rest) = line.strip_prefix(b"\x1bP") {
        let Some(end_idx) = rest.iter().position(|byte| *byte == b'p') else {
            break;
        };
        line = &rest[end_idx + 1..];
    }

    while let Some(rest) = line.strip_suffix(b"\x1b\\") {
        line = rest;
    }

    line
}

pub(crate) fn capture_full_pane_args<'a>(pane_id: &'a str, start_row: &'a str) -> [&'a str; 11] {
    // Full-history hydration does not rely on tmux viewport cursor coordinates.
    // Use `-J` here so soft-wrapped rows are rejoined and do not become hard
    // line breaks after restart when pane width differs at attach time.
    [
        "capture-pane",
        "-p",
        "-e",
        "-C",
        "-J",
        "-S",
        start_row,
        "-E",
        "-",
        "-t",
        pane_id,
    ]
}

pub(crate) fn parse_exit_reason(line: &[u8]) -> Option<String> {
    std::str::from_utf8(line)
        .ok()
        .and_then(|value| value.strip_prefix("%exit"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn parse_output_notification(line: &[u8]) -> Option<(String, Vec<u8>)> {
    if let Some(rest) = line.strip_prefix(b"%output ") {
        let split = rest.iter().position(|byte| *byte == b' ')?;
        let pane_id = String::from_utf8(rest[..split].to_vec()).ok()?;
        let payload = &rest[split + 1..];
        return Some((
            pane_id,
            sanitize_tmux_payload(unescape_tmux_payload(payload)),
        ));
    }

    if let Some(rest) = line.strip_prefix(b"%extended-output ") {
        let colon_idx = rest.iter().position(|byte| *byte == b':')?;
        let header = &rest[..colon_idx];
        let mut header_parts = header.split(|byte| byte.is_ascii_whitespace());
        let pane_id = String::from_utf8(header_parts.next()?.to_vec()).ok()?;
        let mut payload = &rest[colon_idx + 1..];
        if let Some(b' ') = payload.first() {
            payload = &payload[1..];
        }
        return Some((
            pane_id,
            sanitize_tmux_payload(unescape_tmux_payload(payload)),
        ));
    }

    None
}

pub(crate) fn is_refresh_notification(line: &[u8]) -> bool {
    [
        b"%layout-change".as_slice(),
        b"%window-add".as_slice(),
        b"%window-close".as_slice(),
        b"%window-renamed".as_slice(),
        b"%window-pane-changed".as_slice(),
        b"%session-window-changed".as_slice(),
        b"%session-changed".as_slice(),
        b"%sessions-changed".as_slice(),
        b"%unlinked-window-add".as_slice(),
        b"%unlinked-window-close".as_slice(),
        b"%unlinked-window-renamed".as_slice(),
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

pub(crate) fn unescape_tmux_payload(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len());
    let mut index = 0;

    while index < payload.len() {
        if payload[index] == b'\\' && index + 3 < payload.len() {
            let oct = &payload[index + 1..index + 4];
            if oct.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let value = ((oct[0] - b'0') << 6) | ((oct[1] - b'0') << 3) | (oct[2] - b'0');
                output.push(value);
                index += 4;
                continue;
            }
        }

        output.push(payload[index]);
        index += 1;
    }

    output
}

fn normalize_capture_payload(input: Vec<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() + (input.len() / 4));
    for byte in input {
        if byte == b'\n' {
            if !matches!(output.last(), Some(b'\r')) {
                output.push(b'\r');
            }
            output.push(b'\n');
        } else {
            output.push(byte);
        }
    }
    output
}

pub(crate) fn sanitize_tmux_payload(input: Vec<u8>) -> Vec<u8> {
    strip_legacy_title_sequences(normalize_capture_payload(input))
}

pub(crate) fn strip_legacy_title_sequences(input: Vec<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;

    while index < input.len() {
        if input[index] == 0x1b && index + 1 < input.len() && input[index + 1] == b'k' {
            index += 2;
            while index < input.len() {
                if input[index] == 0x07 {
                    index += 1;
                    break;
                }
                if input[index] == 0x1b && index + 1 < input.len() && input[index + 1] == b'\\' {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        output.push(input[index]);
        index += 1;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{
        capture_full_pane_args, parse_output_notification, strip_legacy_title_sequences,
        unescape_tmux_payload,
    };

    fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    #[test]
    fn capture_full_pane_args_match_expected_shape_with_joined_wraps_and_bounded_rows() {
        let args = capture_full_pane_args("%1", "-2060");
        assert_eq!(
            args,
            [
                "capture-pane",
                "-p",
                "-e",
                "-C",
                "-J",
                "-S",
                "-2060",
                "-E",
                "-",
                "-t",
                "%1",
            ]
        );
        assert_ne!(args[6], "-");
    }

    #[test]
    fn output_unescape_decodes_octal_sequences() {
        let decoded = unescape_tmux_payload(b"hello\\040world\\015\\012");
        assert_eq!(decoded, b"hello world\r\n");
    }

    #[test]
    fn parse_output_handles_standard_output_line() {
        let (pane, bytes) = parse_output_notification(b"%output %3 hi\\012").expect("output");
        assert_eq!(pane, "%3");
        assert_eq!(bytes, b"hi\r\n");
    }

    #[test]
    fn parse_output_preserves_crlf_without_double_insert() {
        let (pane, bytes) =
            parse_output_notification(b"%output %5 hi\\015\\012there\\012").expect("output");
        assert_eq!(pane, "%5");
        assert_eq!(bytes, b"hi\r\nthere\r\n");
    }

    #[test]
    fn parse_output_handles_prompt_repaint_fragments() {
        let fragments: [&[u8]; 6] = [
            b"%output %77 c",
            b"%output %77 \\010cd\\040Desk",
            b"%output %77 \\010\\010\\010\\010\\010\\010\\010\\033[32mc\\033[32md\\033[39m\\033[5C",
            b"%output %77 \\015\\015\\012",
            b"%output %77 \\033kcd\\033\\134",
            b"%output %77 cd:\\040no\\040such\\040file\\040or\\040directory:\\040Desk\\015\\012",
        ];

        let mut merged = Vec::new();
        for fragment in fragments {
            let (pane, bytes) = parse_output_notification(fragment).expect("output");
            assert_eq!(pane, "%77");
            merged.extend_from_slice(&bytes);
        }

        assert!(bytes_contains(
            &merged,
            b"cd: no such file or directory: Desk\r\n"
        ));
        assert!(bytes_contains(&merged, b"\r\r\n"));
        assert!(!bytes_contains(&merged, b"cdcd:"));
        assert!(!bytes_contains(&merged, b"czsh:"));
    }

    #[test]
    fn parse_output_preserves_erase_heavy_suffix_spacing() {
        let (_, bytes) = parse_output_notification(
            b"%output %7 error\\015\\012\\040\\040\\040\\040\\040\\040\\040\\040\\015\\015",
        )
        .expect("output");
        assert_eq!(bytes, b"error\r\n        \r\r");
    }

    #[test]
    fn parse_output_strips_legacy_title_sequence() {
        let (_, bytes) =
            parse_output_notification(b"%output %9 \\033kcd\\033\\134").expect("output");
        assert!(bytes.is_empty());
    }

    #[test]
    fn strip_legacy_title_sequence_preserves_surrounding_text() {
        let sanitized = strip_legacy_title_sequences(b"left\x1bkmy-title\x1b\\right".to_vec());
        assert_eq!(sanitized, b"leftright");
    }
}
