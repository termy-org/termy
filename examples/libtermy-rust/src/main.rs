use std::{thread, time::Duration};

use termy_core::{
    Terminal, TerminalClipboardTarget, TerminalReplyHost, TerminalSize,
    load_config_from_default_path,
};

struct EmptyClipboard;

impl TerminalReplyHost for EmptyClipboard {
    fn load_clipboard(&mut self, _target: TerminalClipboardTarget) -> Option<String> {
        None
    }
}

fn main() -> anyhow::Result<()> {
    let loaded_config = load_config_from_default_path()?;
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
        Some(&loaded_config.runtime_config),
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
