use std::{thread, time::Duration};

use termy_core::{
    Terminal, TerminalClipboardTarget, TerminalReplyHost, TerminalRuntimeConfig, TerminalSize,
};

struct EmptyClipboard;

impl TerminalReplyHost for EmptyClipboard {
    fn load_clipboard(&mut self, _target: TerminalClipboardTarget) -> Option<String> {
        None
    }
}

fn main() -> anyhow::Result<()> {
    let terminal = Terminal::new(
        TerminalSize {
            cols: 24,
            rows: 4,
            cell_width: 9.0,
            cell_height: 18.0,
        },
        None,
        None,
        None,
        Some(&TerminalRuntimeConfig::default()),
        Some("printf 'hello from libtermy'"),
    )?;

    thread::sleep(Duration::from_millis(100));
    let _ = terminal.drain_events(&mut EmptyClipboard);
    let frame = terminal.snapshot();

    for row in 0..usize::from(frame.rows) {
        let line = frame
            .cells
            .iter()
            .filter(|cell| cell.row == row)
            .map(|cell| cell.char)
            .collect::<String>();
        println!("{}", line.trim_end());
    }

    Ok(())
}
