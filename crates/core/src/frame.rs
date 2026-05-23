use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    term::{Term, cell::Flags, color::Colors},
    vte::ansi::{Color as AnsiColor, NamedColor, Rgb as AnsiRgb},
};

use crate::{
    protocol::TerminalQueryColors,
    runtime::{TerminalCursorState, TerminalSize, cursor_state_from_term},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermyCell {
    pub col: usize,
    pub row: usize,
    pub char: char,
    pub fg: TermyColor,
    pub bg: TermyColor,
    pub uses_terminal_default_bg: bool,
    pub bold: bool,
    pub render_text: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermyFrame {
    pub cols: u16,
    pub rows: u16,
    pub cells: Vec<TermyCell>,
    pub cursor: Option<TerminalCursorState>,
    pub display_offset: usize,
    pub history_size: usize,
}

fn rgba(rgb: AnsiRgb) -> TermyColor {
    TermyColor {
        r: rgb.r,
        g: rgb.g,
        b: rgb.b,
        a: 255,
    }
}

fn color_to_rgba(
    color: AnsiColor,
    live_colors: &Colors,
    query_colors: TerminalQueryColors,
) -> TermyColor {
    match color {
        AnsiColor::Spec(rgb) => rgba(rgb),
        AnsiColor::Indexed(index) => query_colors
            .resolve_color(live_colors, usize::from(index))
            .map(rgba)
            .unwrap_or_default(),
        AnsiColor::Named(name) => query_colors
            .resolve_color(live_colors, name as usize)
            .map_or_else(|| rgba(query_colors.foreground), rgba),
    }
}

fn bold_foreground_color(color: AnsiColor) -> AnsiColor {
    match color {
        AnsiColor::Named(NamedColor::Black) => AnsiColor::Named(NamedColor::BrightBlack),
        AnsiColor::Named(NamedColor::Red) => AnsiColor::Named(NamedColor::BrightRed),
        AnsiColor::Named(NamedColor::Green) => AnsiColor::Named(NamedColor::BrightGreen),
        AnsiColor::Named(NamedColor::Yellow) => AnsiColor::Named(NamedColor::BrightYellow),
        AnsiColor::Named(NamedColor::Blue) => AnsiColor::Named(NamedColor::BrightBlue),
        AnsiColor::Named(NamedColor::Magenta) => AnsiColor::Named(NamedColor::BrightMagenta),
        AnsiColor::Named(NamedColor::Cyan) => AnsiColor::Named(NamedColor::BrightCyan),
        AnsiColor::Named(NamedColor::White) => AnsiColor::Named(NamedColor::BrightWhite),
        _ => color,
    }
}

fn default_cell(
    col: usize,
    row: usize,
    live_colors: &Colors,
    query_colors: TerminalQueryColors,
) -> TermyCell {
    TermyCell {
        col,
        row,
        char: ' ',
        fg: color_to_rgba(
            AnsiColor::Named(NamedColor::Foreground),
            live_colors,
            query_colors,
        ),
        bg: color_to_rgba(
            AnsiColor::Named(NamedColor::Background),
            live_colors,
            query_colors,
        ),
        uses_terminal_default_bg: true,
        bold: false,
        render_text: false,
    }
}

pub(crate) fn snapshot_from_term<T: EventListener>(
    term: &Term<T>,
    size: TerminalSize,
    query_colors: TerminalQueryColors,
) -> TermyFrame {
    let cols = usize::from(size.cols);
    let rows = usize::from(size.rows);
    let live_colors = term.colors();
    let mut cells = Vec::with_capacity(cols.saturating_mul(rows));
    for row in 0..rows {
        for col in 0..cols {
            cells.push(default_cell(col, row, live_colors, query_colors));
        }
    }

    let content = term.renderable_content();
    for indexed_cell in content.display_iter {
        let row = indexed_cell.point.line.0 + content.display_offset as i32;
        if row < 0 {
            continue;
        }
        let row = row as usize;
        let col = indexed_cell.point.column.0;
        if row >= rows || col >= cols {
            continue;
        }

        let cell = indexed_cell.cell;
        let mut fg = cell.fg;
        let mut bg = cell.bg;
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        // Compute default-bg *after* the inverse swap: a reverse-video cell
        // (e.g. Ink/Claude Code's cursor) paints with the default *foreground*
        // color, so it is no longer the terminal's default background and must
        // be drawn rather than skipped/made transparent.
        let uses_terminal_default_bg = matches!(bg, AnsiColor::Named(NamedColor::Background));
        if cell.flags.contains(Flags::BOLD) {
            fg = bold_foreground_color(fg);
        }

        let mut fg = color_to_rgba(fg, live_colors, query_colors);
        if cell.flags.contains(Flags::DIM) {
            fg.r /= 2;
            fg.g /= 2;
            fg.b /= 2;
        }

        let idx = row
            .checked_mul(cols)
            .and_then(|base| base.checked_add(col))
            .expect("frame cell index must fit usize");
        cells[idx] = TermyCell {
            col,
            row,
            char: cell.c,
            fg,
            bg: color_to_rgba(bg, live_colors, query_colors),
            uses_terminal_default_bg,
            bold: cell.flags.contains(Flags::BOLD),
            render_text: !cell.flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ) && cell.c != '\0'
                && !cell.c.is_control(),
        };
    }

    let grid = term.grid();
    TermyFrame {
        cols: size.cols,
        rows: size.rows,
        cells,
        cursor: cursor_state_from_term(term),
        display_offset: grid.display_offset(),
        history_size: grid.history_size(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{event::VoidListener, term::Config as TermConfig, vte::ansi};

    #[test]
    fn snapshot_contains_visible_output() {
        let size = TerminalSize {
            cols: 4,
            rows: 2,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut term = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"ok");

        let frame = snapshot_from_term(&term, size, TerminalQueryColors::default());

        assert_eq!(frame.cols, 4);
        assert_eq!(frame.rows, 2);
        assert_eq!(frame.cells[0].char, 'o');
        assert_eq!(frame.cells[1].char, 'k');
        assert_eq!(frame.cells.len(), 8);
    }

    #[test]
    fn snapshot_brightens_bold_named_foreground_colors() {
        let size = TerminalSize {
            cols: 2,
            rows: 1,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut term = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"\x1b[31;1mX");

        let frame = snapshot_from_term(&term, size, TerminalQueryColors::default());

        assert_eq!(
            frame.cells[0].fg,
            TermyColor {
                r: 0xff,
                g: 0x00,
                b: 0x00,
                a: 255,
            }
        );
        assert!(frame.cells[0].bold);
    }

    #[test]
    fn snapshot_inverse_default_cell_paints_background() {
        // Ink/Claude Code render the cursor as a reverse-video cell with the
        // terminal's default colors. After the inverse swap its background is
        // the default foreground, so it must NOT be flagged as default-bg or
        // the renderer skips it and the cursor disappears.
        let size = TerminalSize {
            cols: 2,
            rows: 1,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut term = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"\x1b[7mX");

        let frame = snapshot_from_term(&term, size, TerminalQueryColors::default());

        assert!(!frame.cells[0].uses_terminal_default_bg);
        // Inverse swaps fg/bg, so the cell background is the default foreground.
        let default_fg = color_to_rgba(
            AnsiColor::Named(NamedColor::Foreground),
            term.colors(),
            TerminalQueryColors::default(),
        );
        assert_eq!(frame.cells[0].bg, default_fg);
    }

    #[test]
    fn snapshot_marks_explicit_backgrounds() {
        let size = TerminalSize {
            cols: 2,
            rows: 1,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut term = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"\x1b[44mX");

        let frame = snapshot_from_term(&term, size, TerminalQueryColors::default());

        assert!(!frame.cells[0].uses_terminal_default_bg);
        assert_eq!(
            frame.cells[0].bg,
            TermyColor {
                r: 0x00,
                g: 0x00,
                b: 0xee,
                a: 255,
            }
        );
    }
}
