use alacritty_terminal::{
    event::VoidListener,
    grid::{Dimensions, Scroll},
    sync::FairMutex,
    term::{Config as TermConfig, Term, TermMode},
    vte::ansi,
};

use crate::runtime::TerminalSize;

struct PaneTerminalInner {
    term: Term<VoidListener>,
    size: TerminalSize,
    scrollback_history: usize,
}

/// In-memory terminal emulator for a tmux pane.
pub struct PaneTerminal {
    inner: FairMutex<PaneTerminalInner>,
    parser: FairMutex<ansi::Processor>,
}

impl PaneTerminal {
    pub fn new(size: TerminalSize, scrollback_history: usize) -> Self {
        let mut config = TermConfig::default();
        config.scrolling_history = scrollback_history;

        let term = Term::new(config, &size, VoidListener);
        Self {
            inner: FairMutex::new(PaneTerminalInner {
                term,
                size,
                scrollback_history,
            }),
            parser: FairMutex::new(ansi::Processor::new()),
        }
    }

    pub fn feed_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let mut parser = self.parser.lock();
        let mut inner = self.inner.lock();
        parser.advance(&mut inner.term, bytes);
    }

    pub fn resize(&self, new_size: TerminalSize) {
        let mut inner = self.inner.lock();
        inner.size = new_size;
        inner.term.resize(new_size);
    }

    pub fn size(&self) -> TerminalSize {
        self.inner.lock().size
    }

    pub fn with_term<R>(&self, f: impl FnOnce(&Term<VoidListener>) -> R) -> R {
        let inner = self.inner.lock();
        f(&inner.term)
    }

    pub fn scroll_display(&self, delta_lines: i32) -> bool {
        if delta_lines == 0 {
            return false;
        }

        let mut inner = self.inner.lock();
        let old_offset = inner.term.grid().display_offset();
        inner.term.scroll_display(Scroll::Delta(delta_lines));
        inner.term.grid().display_offset() != old_offset
    }

    pub fn scroll_to_bottom(&self) -> bool {
        let mut inner = self.inner.lock();
        let old_offset = inner.term.grid().display_offset();
        if old_offset == 0 {
            return false;
        }
        inner.term.scroll_display(Scroll::Bottom);
        true
    }

    pub fn scroll_state(&self) -> (usize, usize) {
        let inner = self.inner.lock();
        let grid = inner.term.grid();
        (grid.display_offset(), grid.history_size())
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        let inner = self.inner.lock();
        let cursor = inner.term.grid().cursor.point;
        let row = usize::try_from(cursor.line.0).unwrap_or(0);
        (cursor.column.0, row)
    }

    pub fn set_scrollback_history(&self, history_size: usize) {
        let mut inner = self.inner.lock();
        if inner.scrollback_history == history_size {
            return;
        }
        let mut config = TermConfig::default();
        config.scrolling_history = history_size;
        inner.term.set_options(config);
        inner.scrollback_history = history_size;
    }

    pub fn bracketed_paste_mode(&self) -> bool {
        let inner = self.inner.lock();
        inner.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn alternate_screen_mode(&self) -> bool {
        let inner = self.inner.lock();
        inner.term.mode().contains(TermMode::ALT_SCREEN)
    }
}

impl Dimensions for PaneTerminal {
    fn total_lines(&self) -> usize {
        self.size().rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.size().rows as usize
    }

    fn columns(&self) -> usize {
        self.size().cols as usize
    }

    fn last_column(&self) -> alacritty_terminal::index::Column {
        alacritty_terminal::index::Column(self.size().cols.saturating_sub(1) as usize)
    }

    fn bottommost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line((self.size().rows as i32) - 1)
    }

    fn topmost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(0)
    }
}

#[cfg(test)]
mod tests {
    use super::PaneTerminal;
    use crate::runtime::TerminalSize;

    fn visible_viewport_text(terminal: &PaneTerminal) -> String {
        let size = terminal.size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        let mut grid = vec![vec![' '; cols]; rows];

        terminal.with_term(|term| {
            let content = term.renderable_content();
            for cell in content.display_iter {
                let row = cell.point.line.0 + content.display_offset as i32;
                if row < 0 {
                    continue;
                }
                let row = row as usize;
                let col = cell.point.column.0;
                if row >= rows || col >= cols {
                    continue;
                }

                let c = cell.cell.c;
                if c != '\0' && !c.is_control() {
                    grid[row][col] = c;
                }
            }
        });

        grid.into_iter()
            .map(|row| {
                let line: String = row.into_iter().collect();
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn feed_output_handles_tmux_prompt_repaint_without_prefix_duplication() {
        let terminal = PaneTerminal::new(
            TerminalSize {
                cols: 120,
                rows: 10,
                ..TerminalSize::default()
            },
            2000,
        );

        terminal.feed_output(
            b"c\x08cd Desk\x08\x08\x08\x08\x08\x08\x08\x1b[32mc\x1b[32md\x1b[39m\x1b[5C\r\r\n",
        );
        terminal.feed_output(b"cd: no such file or directory: Desk\r\n");
        terminal.feed_output(b"c\x08\x1b[4mc\x1b[24m\r\r\n");
        terminal.feed_output(b"zsh: command not found: c\r\n");

        let visible = visible_viewport_text(&terminal);
        assert!(visible.contains("cd: no such file or directory: Desk"));
        assert!(visible.contains("zsh: command not found: c"));
        assert!(!visible.contains("cdcd:"));
        assert!(!visible.contains("czsh:"));
    }
}
