use crate::frame::TermyFrame;
use termy_search::{SearchConfig, SearchEngine, SearchMode};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermySearchMatch {
    pub row: usize,
    pub start_col: usize,
    /// Inclusive end column for Swift/FFI consumers.
    pub end_col: usize,
    pub line: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermySearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
}

pub fn search_frame(frame: &TermyFrame, query: &str) -> Vec<TermySearchMatch> {
    search_frame_with_options(frame, query, TermySearchOptions::default())
}

pub fn search_frame_with_options(
    frame: &TermyFrame,
    query: &str,
    options: TermySearchOptions,
) -> Vec<TermySearchMatch> {
    if query.is_empty() || frame.cols == 0 {
        return Vec::new();
    }

    let cols = usize::from(frame.cols);
    let rows = usize::from(frame.rows);
    search_lines(
        (0..rows).map(|row| (row, line_text(frame, row, cols))),
        query,
        options,
    )
}

pub(crate) fn search_lines(
    lines: impl IntoIterator<Item = (usize, String)>,
    query: &str,
    options: TermySearchOptions,
) -> Vec<TermySearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let mut engine = SearchEngine::new(SearchConfig {
        case_sensitive: options.case_sensitive,
        mode: if options.regex {
            SearchMode::Regex
        } else {
            SearchMode::Literal
        },
    });
    if engine.set_pattern(query).is_err() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for (row, line) in lines {
        for search_match in engine.search_line(row as i32, &line) {
            if search_match.end_col <= search_match.start_col {
                continue;
            }
            matches.push(TermySearchMatch {
                row,
                start_col: search_match.start_col,
                end_col: search_match.end_col.saturating_sub(1),
                line: line.clone(),
            });
        }
    }

    matches
}

fn line_text(frame: &TermyFrame, row: usize, cols: usize) -> String {
    let start = row.saturating_mul(cols);
    let end = start.saturating_add(cols);
    if end > frame.cells.len() {
        return String::new();
    }

    frame.cells[start..end]
        .iter()
        .map(|cell| if cell.render_text { cell.char } else { ' ' })
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{TermyCell, TermyColor};

    #[test]
    fn searches_visible_frame_rows_case_insensitively() {
        let frame = frame_from_rows(12, &["hello", "world hello"]);

        let matches = search_frame(&frame, "HELLO");

        assert_eq!(
            matches,
            vec![
                TermySearchMatch {
                    row: 0,
                    start_col: 0,
                    end_col: 4,
                    line: "hello".to_string(),
                },
                TermySearchMatch {
                    row: 1,
                    start_col: 6,
                    end_col: 10,
                    line: "world hello".to_string(),
                },
            ]
        );
    }

    #[test]
    fn empty_query_returns_no_matches() {
        assert!(search_frame(&frame_from_rows(4, &["abc"]), "").is_empty());
    }

    #[test]
    fn search_options_can_enable_case_sensitive_matching() {
        let frame = frame_from_rows(12, &["Hello HELLO"]);

        let matches = search_frame_with_options(
            &frame,
            "HELLO",
            TermySearchOptions {
                case_sensitive: true,
                regex: false,
            },
        );

        assert_eq!(
            matches,
            vec![TermySearchMatch {
                row: 0,
                start_col: 6,
                end_col: 10,
                line: "Hello HELLO".to_string(),
            }]
        );
    }

    #[test]
    fn search_options_can_enable_regex_matching() {
        let frame = frame_from_rows(16, &["foo 123 bar"]);

        let matches = search_frame_with_options(
            &frame,
            r"\d+",
            TermySearchOptions {
                case_sensitive: false,
                regex: true,
            },
        );

        assert_eq!(
            matches,
            vec![TermySearchMatch {
                row: 0,
                start_col: 4,
                end_col: 6,
                line: "foo 123 bar".to_string(),
            }]
        );
    }

    #[test]
    fn invalid_regex_returns_no_matches() {
        let frame = frame_from_rows(12, &["hello"]);

        let matches = search_frame_with_options(
            &frame,
            "[",
            TermySearchOptions {
                case_sensitive: false,
                regex: true,
            },
        );

        assert!(matches.is_empty());
    }

    fn frame_from_rows(cols: u16, rows: &[&str]) -> TermyFrame {
        let color = TermyColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let mut cells = Vec::new();
        for (row, text) in rows.iter().enumerate() {
            let mut chars = text.chars();
            for col in 0..usize::from(cols) {
                let char = chars.next().unwrap_or(' ');
                cells.push(TermyCell {
                    col,
                    row,
                    char,
                    fg: color,
                    bg: TermyColor::default(),
                    uses_terminal_default_bg: true,
                    bold: false,
                    render_text: char != ' ',
                });
            }
        }

        TermyFrame {
            cols,
            rows: rows.len() as u16,
            cells,
            cursor: None,
            display_offset: 0,
            history_size: 0,
        }
    }
}
