use super::super::*;

impl TerminalView {
    pub(crate) fn truncate_tab_title(title: &str) -> String {
        // Keep titles single-line so shell-provided newlines do not break tab layout.
        let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.chars().count() > MAX_TAB_TITLE_CHARS {
            return normalized.chars().take(MAX_TAB_TITLE_CHARS).collect();
        }
        normalized
    }

    fn is_path_like_tab_title(title: &str) -> bool {
        title.contains('/') || title.contains('\\')
    }

    fn squeezed_path_tab_label_for_preserved_chars(
        chars: &[char],
        basename_len: usize,
        preserved_chars: usize,
    ) -> String {
        if chars.is_empty() {
            return String::new();
        }

        if preserved_chars == 0 {
            return "...".to_string();
        }

        let (head_chars, tail_chars) = if preserved_chars == 1 {
            (0, 1)
        } else {
            let max_tail_chars = preserved_chars - 1;
            let min_tail_chars = preserved_chars / 2;
            let preferred_tail_chars = (basename_len + 1).min(max_tail_chars);
            let tail_chars = preferred_tail_chars.max(min_tail_chars).min(max_tail_chars);
            (preserved_chars.saturating_sub(tail_chars), tail_chars)
        };

        let mut formatted = String::with_capacity(head_chars + 3 + tail_chars);
        for ch in chars.iter().take(head_chars) {
            formatted.push(*ch);
        }
        formatted.push_str("...");
        for ch in chars
            .iter()
            .skip(chars.len().saturating_sub(tail_chars))
            .take(tail_chars)
        {
            formatted.push(*ch);
        }

        formatted
    }

    fn fitting_dots_for_width<F>(available_text_px: f32, measure_text_px: &mut F) -> String
    where
        F: FnMut(&str) -> f32,
    {
        if available_text_px <= f32::EPSILON {
            return String::new();
        }

        for dots in ["...", "..", "."] {
            if measure_text_px(dots) <= available_text_px {
                return dots.to_string();
            }
        }

        String::new()
    }

    pub(crate) fn format_tab_label_for_render_measured<F>(
        title: &str,
        available_text_px: f32,
        mut measure_text_px: F,
    ) -> String
    where
        F: FnMut(&str) -> f32,
    {
        let available_text_px = if available_text_px.is_finite() {
            available_text_px.max(0.0)
        } else {
            0.0
        };
        if title.is_empty() || available_text_px <= f32::EPSILON {
            return String::new();
        }

        if measure_text_px(title) <= available_text_px {
            return title.to_string();
        }

        if !Self::is_path_like_tab_title(title) {
            // Non-path titles keep end-truncation behavior through render-level text ellipsis.
            return title.to_string();
        }

        let chars: Vec<char> = title.chars().collect();
        if chars.is_empty() {
            return String::new();
        }
        let basename_len = chars
            .iter()
            .rposition(|ch| *ch == '/' || *ch == '\\')
            .map_or(chars.len(), |index| chars.len().saturating_sub(index + 1));
        let candidate_for = |preserved_chars: usize| {
            Self::squeezed_path_tab_label_for_preserved_chars(&chars, basename_len, preserved_chars)
        };

        let mut low = 0usize;
        let mut high = chars.len();
        while low < high {
            let mid = (low + high + 1) / 2;
            let candidate = candidate_for(mid);
            if measure_text_px(candidate.as_str()) <= available_text_px {
                low = mid;
            } else {
                high = mid.saturating_sub(1);
            }
        }

        let fitted = candidate_for(low);
        if measure_text_px(fitted.as_str()) <= available_text_px {
            fitted
        } else {
            Self::fitting_dots_for_width(available_text_px, &mut measure_text_px)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_text_width(text: &str) -> f32 {
        text.chars()
            .map(|ch| match ch {
                '/' | '\\' => 5.0,
                '.' => 3.5,
                'i' | 'l' | '1' => 4.5,
                'W' | 'M' => 9.0,
                _ => 7.0,
            })
            .sum()
    }

    #[test]
    fn measured_tab_title_fit_keeps_exact_fit_path_untruncated() {
        let title = "~/Desktop";
        let width = synthetic_text_width(title);

        assert_eq!(
            TerminalView::format_tab_label_for_render_measured(title, width, synthetic_text_width),
            title
        );
    }

    #[test]
    fn measured_tab_title_fit_middle_squeezes_path_titles() {
        let title = "~/Desktop/claudeCode/claude-code-provider-proxy/docs";
        let available = synthetic_text_width("~/Desktop/.../docs");
        let formatted = TerminalView::format_tab_label_for_render_measured(
            title,
            available,
            synthetic_text_width,
        );

        assert!(formatted.contains("..."));
        assert!(formatted.starts_with("~/"));
        assert!(formatted.ends_with("/docs"));
        assert!(synthetic_text_width(&formatted) <= available);
    }

    #[test]
    fn measured_tab_title_fit_returns_dots_for_tiny_widths() {
        let title = "~/Desktop/claudeCode/claude-code-provider-proxy/docs";
        assert_eq!(
            TerminalView::format_tab_label_for_render_measured(
                title,
                synthetic_text_width("..."),
                synthetic_text_width,
            ),
            "..."
        );
        assert_eq!(
            TerminalView::format_tab_label_for_render_measured(
                title,
                synthetic_text_width(".."),
                synthetic_text_width,
            ),
            ".."
        );
        assert_eq!(
            TerminalView::format_tab_label_for_render_measured(title, 0.0, synthetic_text_width),
            ""
        );
    }

    #[test]
    fn measured_tab_title_fit_leaves_non_path_titles_for_end_truncation() {
        let title = "cargo test --workspace --all-features";
        assert_eq!(
            TerminalView::format_tab_label_for_render_measured(
                title,
                synthetic_text_width("cargo test"),
                synthetic_text_width,
            ),
            "cargo test --workspace --all-features"
        );
    }

    #[test]
    fn measured_tab_title_fit_never_overflows_available_width() {
        let title = "~/Desktop/claudeCode/claude-code-provider-proxy/docs/test2/test4/test4";
        let available = synthetic_text_width("~/Desktop/.../test4");
        let formatted = TerminalView::format_tab_label_for_render_measured(
            title,
            available,
            synthetic_text_width,
        );

        assert!(synthetic_text_width(&formatted) <= available);
    }
}
