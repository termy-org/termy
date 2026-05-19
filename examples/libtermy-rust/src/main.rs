use std::{thread, time::Duration};

use termy_core::{
    Terminal, TerminalClipboardTarget, TerminalReplyHost, TerminalSize, measure_cell_from_config,
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
    let cell_metrics = measure_cell_from_config(&loaded_config.app_config);
    let terminal = Terminal::new(
        TerminalSize {
            cols: 24,
            rows: 4,
            cell_width: cell_metrics.cell_width,
            cell_height: cell_metrics.cell_height,
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
