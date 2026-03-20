use alacritty_terminal::{
    event::VoidListener,
    grid::{Dimensions, Scroll},
    sync::FairMutex,
    term::{Term, TermMode},
    vte::ansi,
};
use std::sync::Arc;

use crate::mouse_protocol::TerminalMouseMode;
use crate::runtime::{
    TerminalCursorState, TerminalDamageSnapshot, TerminalOptions, TerminalSize,
    cursor_position_from_term, cursor_state_from_term, take_term_damage_snapshot,
    termmode_to_terminal_mouse_mode,
};
use crate::keyboard::TerminalKeyboardMode;

struct PaneTerminalInner {
    term: Arc<FairMutex<Term<VoidListener>>>,
    size: TerminalSize,
}

/// In-memory terminal emulator for a tmux pane.
pub struct PaneTerminal {
    inner: FairMutex<PaneTerminalInner>,
    parser: FairMutex<ansi::Processor>,
}

impl PaneTerminal {
    fn normalized_size(size: TerminalSize) -> TerminalSize {
        TerminalSize {
            cols: size.cols.max(1),
            rows: size.rows.max(1),
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        }
    }

    pub fn new(size: TerminalSize, options: TerminalOptions) -> Self {
        let size = Self::normalized_size(size);
        let config = options.term_config();

        let term = Arc::new(FairMutex::new(Term::new(config, &size, VoidListener)));
        Self {
            inner: FairMutex::new(PaneTerminalInner { term, size }),
            parser: FairMutex::new(ansi::Processor::new()),
        }
    }

    fn cloned_term_arc(&self) -> Arc<FairMutex<Term<VoidListener>>> {
        let inner = self.inner.lock();
        inner.term.clone()
    }

    pub fn feed_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        let mut parser = self.parser.lock();
        let term = self.cloned_term_arc();
        let mut term = term.lock();
        parser.advance(&mut *term, bytes);
    }

    pub fn resize(&self, new_size: TerminalSize) {
        let new_size = Self::normalized_size(new_size);
        let term = self.cloned_term_arc();
        let mut term = term.lock();
        let mut inner = self.inner.lock();
        // Keep cached pane size and the backing terminal dimensions synchronized
        // under the same critical section so concurrent readers never observe
        // a partially-applied resize.
        inner.size = new_size;
        term.resize(new_size);
    }

    pub fn size(&self) -> TerminalSize {
        self.inner.lock().size
    }

    pub fn with_term<R>(&self, f: impl FnOnce(&Term<VoidListener>) -> R) -> R {
        let term = self.cloned_term_arc();
        // Run callback outside the outer state lock so callbacks can safely call
        // back into PaneTerminal APIs (for example size()) without lock inversion.
        let term = term.lock();
        f(&term)
    }

    fn with_term_mut<R>(
        &self,
        f: impl FnOnce(&mut Term<VoidListener>, &mut PaneTerminalInner) -> R,
    ) -> R {
        let term = self.cloned_term_arc();
        let mut term = term.lock();
        let mut inner = self.inner.lock();
        let result = f(&mut term, &mut inner);
        // Keep cached dimensions aligned with any in-place terminal mutation so
        // PaneTerminalInner and Term cannot drift.
        inner.size.cols = u16::try_from(term.grid().columns()).unwrap_or(u16::MAX);
        inner.size.rows = u16::try_from(term.grid().screen_lines()).unwrap_or(u16::MAX);
        result
    }

    pub fn take_damage_snapshot(&self) -> TerminalDamageSnapshot {
        self.with_term_mut(|term, _inner| take_term_damage_snapshot(term))
    }

    pub fn scroll_display(&self, delta_lines: i32) -> bool {
        if delta_lines == 0 {
            return false;
        }

        let term = self.cloned_term_arc();
        let mut term = term.lock();
        let old_offset = term.grid().display_offset();
        term.scroll_display(Scroll::Delta(delta_lines));
        term.grid().display_offset() != old_offset
    }

    pub fn scroll_to_bottom(&self) -> bool {
        let term = self.cloned_term_arc();
        let mut term = term.lock();
        let old_offset = term.grid().display_offset();
        if old_offset == 0 {
            return false;
        }
        term.scroll_display(Scroll::Bottom);
        true
    }

    pub fn scroll_state(&self) -> (usize, usize) {
        let term = self.cloned_term_arc();
        let term = term.lock();
        let grid = term.grid();
        (grid.display_offset(), grid.history_size())
    }

    pub fn cursor_state(&self) -> Option<TerminalCursorState> {
        let term = self.cloned_term_arc();
        let term = term.lock();
        cursor_state_from_term(&term)
    }

    /// Returns the cursor position regardless of visibility (for IME positioning).
    pub fn cursor_position(&self) -> (usize, usize) {
        let term = self.cloned_term_arc();
        let term = term.lock();
        cursor_position_from_term(&term)
    }

    pub fn set_term_options(&self, options: TerminalOptions) {
        self.with_term_mut(|term, _inner| term.set_options(options.term_config()));
    }

    pub fn bracketed_paste_mode(&self) -> bool {
        let term = self.cloned_term_arc();
        term.lock().mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub fn mouse_mode(&self) -> TerminalMouseMode {
        let term = self.cloned_term_arc();
        let mode = *term.lock().mode();
        termmode_to_terminal_mouse_mode(mode)
    }

    pub fn keyboard_mode(&self) -> TerminalKeyboardMode {
        let term = self.cloned_term_arc();
        TerminalKeyboardMode::from_term_mode(*term.lock().mode())
    }

    pub fn alternate_screen_mode(&self) -> bool {
        let term = self.cloned_term_arc();
        term.lock().mode().contains(TermMode::ALT_SCREEN)
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
        alacritty_terminal::index::Line(i32::from(self.size().rows.saturating_sub(1)))
    }

    fn topmost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(0)
    }
}

#[cfg(test)]
mod tests {
    use super::PaneTerminal;
    use crate::runtime::{TerminalOptions, TerminalSize};
    use alacritty_terminal::grid::Dimensions;

    fn test_term_options(scrollback_history: usize) -> TerminalOptions {
        TerminalOptions {
            scrollback_history,
            ..TerminalOptions::default()
        }
    }

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
            test_term_options(2000),
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

    #[test]
    fn clamps_zero_size_on_new_and_resize() {
        let terminal = PaneTerminal::new(
            TerminalSize {
                cols: 0,
                rows: 0,
                ..TerminalSize::default()
            },
            test_term_options(2000),
        );
        assert_eq!(terminal.size().cols, 1);
        assert_eq!(terminal.size().rows, 1);
        assert_eq!(terminal.last_column().0, 0);
        assert_eq!(terminal.bottommost_line().0, 0);

        terminal.resize(TerminalSize {
            cols: 0,
            rows: 0,
            ..TerminalSize::default()
        });
        assert_eq!(terminal.size().cols, 1);
        assert_eq!(terminal.size().rows, 1);
        assert_eq!(terminal.last_column().0, 0);
        assert_eq!(terminal.bottommost_line().0, 0);
    }

    #[test]
    fn with_term_mut_resyncs_cached_size_after_direct_term_resize() {
        let terminal = PaneTerminal::new(
            TerminalSize {
                cols: 4,
                rows: 3,
                ..TerminalSize::default()
            },
            test_term_options(2000),
        );

        terminal.with_term_mut(|term, _inner| {
            term.resize(TerminalSize {
                cols: 9,
                rows: 7,
                ..TerminalSize::default()
            });
        });

        let size = terminal.size();
        assert_eq!(size.cols, 9);
        assert_eq!(size.rows, 7);
    }

    #[test]
    fn mouse_mode_detects_click_and_sgr_flags_from_output_stream() {
        let terminal = PaneTerminal::new(
            TerminalSize {
                cols: 4,
                rows: 3,
                ..TerminalSize::default()
            },
            test_term_options(2000),
        );

        terminal.feed_output(b"\x1b[?1000h\x1b[?1006h");
        let mode = terminal.mouse_mode();
        assert!(mode.enabled);
        assert!(mode.report_click);
        assert!(mode.sgr_encoding);
        assert!(!mode.report_drag);
        assert!(!mode.report_motion);
    }

    #[test]
    fn mouse_mode_detects_drag_and_motion_flags_from_output_stream() {
        let terminal = PaneTerminal::new(
            TerminalSize {
                cols: 4,
                rows: 3,
                ..TerminalSize::default()
            },
            test_term_options(2000),
        );

        terminal.feed_output(b"\x1b[?1002h");
        let drag_mode = terminal.mouse_mode();
        assert!(drag_mode.enabled);
        assert!(drag_mode.report_drag);
        assert!(!drag_mode.report_motion);

        terminal.feed_output(b"\x1b[?1003h");
        let motion_mode = terminal.mouse_mode();
        assert!(motion_mode.enabled);
        assert!(motion_mode.report_motion);
    }
}
