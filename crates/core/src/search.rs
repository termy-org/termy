use crate::frame::TermyFrame;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermySearchMatch {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub line: String,
}

pub fn search_frame(frame: &TermyFrame, query: &str) -> Vec<TermySearchMatch> {
    if query.is_empty() || frame.cols == 0 {
        return Vec::new();
    }

    let query = query.to_ascii_lowercase();
    let query_len = query.chars().count();
    let cols = usize::from(frame.cols);
    let rows = usize::from(frame.rows);
    let mut matches = Vec::new();

    for row in 0..rows {
        let line = line_text(frame, row, cols);
        let searchable = line.to_ascii_lowercase();
        let mut offset = 0;

        while let Some(byte_index) = searchable[offset..].find(&query) {
            let start_byte = offset + byte_index;
            let start_col = searchable[..start_byte].chars().count();
            let end_col = start_col + query_len.saturating_sub(1);
            matches.push(TermySearchMatch {
                row,
                start_col,
                end_col,
                line: line.clone(),
            });
            offset = start_byte + query.len();
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
