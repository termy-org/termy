#![allow(clippy::missing_safety_doc)]

use std::{ptr, slice, str};

use termy_core::{
    ProgressState, Terminal, TerminalClipboardTarget, TerminalDamageSnapshot, TerminalDirtySpan,
    TerminalEvent, TerminalOptions, TerminalQueryColors, TerminalReplyHost, TerminalRuntimeConfig,
    TerminalSize, TermyCell, TermyColor,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermyFfiStatus {
    Ok = 0,
    Null = 1,
    InvalidUtf8 = 2,
    SpawnFailed = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermyFfiSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: f32,
    pub cell_height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiCell {
    pub col: usize,
    pub row: usize,
    pub codepoint: u32,
    pub fg: TermyFfiColor,
    pub bg: TermyFfiColor,
    pub uses_terminal_default_bg: bool,
    pub bold: bool,
    pub render_text: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiCursor {
    pub visible: bool,
    pub col: usize,
    pub row: usize,
    pub style: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiFrame {
    pub cols: u16,
    pub rows: u16,
    pub cells_ptr: *mut TermyFfiCell,
    pub cells_len: usize,
    pub cells_capacity: usize,
    pub cursor: TermyFfiCursor,
    pub display_offset: usize,
    pub history_size: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiBytes {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiEvent {
    pub kind: u32,
    pub exit_code: i32,
    pub progress_state: u8,
    pub progress_value: u8,
    pub payload: TermyFfiBytes,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiEventBatch {
    pub events_ptr: *mut TermyFfiEvent,
    pub events_len: usize,
    pub events_capacity: usize,
    pub has_more: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiDirtySpan {
    pub row: usize,
    pub left_col: usize,
    pub right_col: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiDamage {
    pub kind: u32,
    pub spans_ptr: *mut TermyFfiDirtySpan,
    pub spans_len: usize,
    pub spans_capacity: usize,
}

pub struct TermyFfiTerminal {
    terminal: Terminal,
}

struct EmptyReplyHost;

impl TerminalReplyHost for EmptyReplyHost {
    fn load_clipboard(&mut self, _target: TerminalClipboardTarget) -> Option<String> {
        None
    }
}

impl From<TermyFfiSize> for TerminalSize {
    fn from(size: TermyFfiSize) -> Self {
        Self {
            cols: size.cols,
            rows: size.rows,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        }
    }
}

impl From<TermyColor> for TermyFfiColor {
    fn from(color: TermyColor) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

impl From<TermyCell> for TermyFfiCell {
    fn from(cell: TermyCell) -> Self {
        Self {
            col: cell.col,
            row: cell.row,
            codepoint: cell.char as u32,
            fg: cell.fg.into(),
            bg: cell.bg.into(),
            uses_terminal_default_bg: cell.uses_terminal_default_bg,
            bold: cell.bold,
            render_text: cell.render_text,
        }
    }
}

fn ffi_bytes_from_vec(mut bytes: Vec<u8>) -> TermyFfiBytes {
    let result = TermyFfiBytes {
        ptr: bytes.as_mut_ptr(),
        len: bytes.len(),
        capacity: bytes.capacity(),
    };
    std::mem::forget(bytes);
    result
}

fn ffi_bytes_from_string(value: String) -> TermyFfiBytes {
    ffi_bytes_from_vec(value.into_bytes())
}

fn progress_parts(progress: ProgressState) -> (u8, u8) {
    match progress {
        ProgressState::Clear => (0, 0),
        ProgressState::InProgress(value) => (1, value),
        ProgressState::Error(value) => (2, value),
        ProgressState::Indeterminate => (3, 0),
        ProgressState::Warning(value) => (4, value),
    }
}

fn ffi_event_from_event(event: TerminalEvent) -> TermyFfiEvent {
    match event {
        TerminalEvent::Wakeup => TermyFfiEvent {
            kind: 1,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::Title(title) => TermyFfiEvent {
            kind: 2,
            payload: ffi_bytes_from_string(title),
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ResetTitle => TermyFfiEvent {
            kind: 3,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::Bell => TermyFfiEvent {
            kind: 4,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::Exit => TermyFfiEvent {
            kind: 5,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ClipboardStore(text) => TermyFfiEvent {
            kind: 6,
            payload: ffi_bytes_from_string(text),
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ShellPromptStart => TermyFfiEvent {
            kind: 7,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ShellCommandStart => TermyFfiEvent {
            kind: 8,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ShellCommandExecuting => TermyFfiEvent {
            kind: 9,
            ..TermyFfiEvent::default()
        },
        TerminalEvent::ShellCommandFinished(code) => TermyFfiEvent {
            kind: 10,
            exit_code: code.unwrap_or(-1),
            ..TermyFfiEvent::default()
        },
        TerminalEvent::Progress(progress) => {
            let (progress_state, progress_value) = progress_parts(progress);
            TermyFfiEvent {
                kind: 11,
                progress_state,
                progress_value,
                ..TermyFfiEvent::default()
            }
        }
        TerminalEvent::WorkingDirectory(path) => TermyFfiEvent {
            kind: 12,
            payload: ffi_bytes_from_string(path),
            ..TermyFfiEvent::default()
        },
    }
}

fn leak_vec<T>(mut vec: Vec<T>) -> (*mut T, usize, usize) {
    let ptr = vec.as_mut_ptr();
    let len = vec.len();
    let capacity = vec.capacity();
    std::mem::forget(vec);
    (ptr, len, capacity)
}

unsafe fn optional_utf8<'a>(ptr: *const u8, len: usize) -> Result<Option<&'a str>, TermyFfiStatus> {
    if ptr.is_null() || len == 0 {
        return Ok(None);
    }

    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    str::from_utf8(bytes)
        .map(Some)
        .map_err(|_| TermyFfiStatus::InvalidUtf8)
}

#[unsafe(no_mangle)]
pub extern "C" fn termy_size_default() -> TermyFfiSize {
    let size = TerminalSize::default();
    TermyFfiSize {
        cols: size.cols,
        rows: size.rows,
        cell_width: size.cell_width,
        cell_height: size.cell_height,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_new(
    size: TermyFfiSize,
    startup_command_ptr: *const u8,
    startup_command_len: usize,
    out_terminal: *mut *mut TermyFfiTerminal,
) -> TermyFfiStatus {
    if out_terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    let startup_command = match unsafe { optional_utf8(startup_command_ptr, startup_command_len) } {
        Ok(value) => value,
        Err(status) => return status,
    };

    let Ok(terminal) = Terminal::new(
        size.into(),
        None,
        None,
        None,
        Some(&TerminalRuntimeConfig::default()),
        startup_command,
    ) else {
        return TermyFfiStatus::SpawnFailed;
    };

    unsafe {
        *out_terminal = Box::into_raw(Box::new(TermyFfiTerminal { terminal }));
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_free(terminal: *mut TermyFfiTerminal) -> TermyFfiStatus {
    if terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        drop(Box::from_raw(terminal));
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_write(
    terminal: *mut TermyFfiTerminal,
    bytes_ptr: *const u8,
    bytes_len: usize,
) -> TermyFfiStatus {
    if terminal.is_null() || bytes_ptr.is_null() {
        return TermyFfiStatus::Null;
    }

    let bytes = unsafe { slice::from_raw_parts(bytes_ptr, bytes_len) };
    unsafe {
        (*terminal).terminal.write(bytes);
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_resize(
    terminal: *mut TermyFfiTerminal,
    size: TermyFfiSize,
) -> TermyFfiStatus {
    if terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        (*terminal).terminal.resize(size.into());
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_set_wakeup_enabled(
    terminal: *mut TermyFfiTerminal,
    enabled: bool,
) -> TermyFfiStatus {
    if terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        (*terminal).terminal.set_wakeup_enabled(enabled);
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_snapshot(
    terminal: *mut TermyFfiTerminal,
    out_frame: *mut TermyFfiFrame,
) -> TermyFfiStatus {
    if terminal.is_null() || out_frame.is_null() {
        return TermyFfiStatus::Null;
    }

    let frame = unsafe { (*terminal).terminal.snapshot() };
    let cells = frame
        .cells
        .into_iter()
        .map(TermyFfiCell::from)
        .collect::<Vec<_>>();
    let (cells_ptr, cells_len, cells_capacity) = leak_vec(cells);
    let cursor = frame
        .cursor
        .map_or_else(TermyFfiCursor::default, |cursor| TermyFfiCursor {
            visible: true,
            col: cursor.col,
            row: cursor.row,
            style: match cursor.style {
                termy_core::TerminalCursorStyle::Line => 1,
                termy_core::TerminalCursorStyle::Block => 2,
            },
        });

    unsafe {
        *out_frame = TermyFfiFrame {
            cols: frame.cols,
            rows: frame.rows,
            cells_ptr,
            cells_len,
            cells_capacity,
            cursor,
            display_offset: frame.display_offset,
            history_size: frame.history_size,
        };
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_frame_free(frame: *mut TermyFfiFrame) -> TermyFfiStatus {
    if frame.is_null() {
        return TermyFfiStatus::Null;
    }

    let frame = unsafe { &mut *frame };
    if !frame.cells_ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(
                frame.cells_ptr,
                frame.cells_len,
                frame.cells_capacity,
            ));
        }
    }
    *frame = TermyFfiFrame::default();
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_take_damage(
    terminal: *mut TermyFfiTerminal,
    out_damage: *mut TermyFfiDamage,
) -> TermyFfiStatus {
    if terminal.is_null() || out_damage.is_null() {
        return TermyFfiStatus::Null;
    }

    let damage = unsafe { (*terminal).terminal.take_damage_snapshot() };
    let result = match damage {
        TerminalDamageSnapshot::Full => TermyFfiDamage {
            kind: 1,
            ..TermyFfiDamage::default()
        },
        TerminalDamageSnapshot::Partial(spans) if spans.is_empty() => TermyFfiDamage::default(),
        TerminalDamageSnapshot::Partial(spans) => {
            let spans = spans
                .into_iter()
                .map(
                    |TerminalDirtySpan {
                         row,
                         left_col,
                         right_col,
                     }| TermyFfiDirtySpan {
                        row,
                        left_col,
                        right_col,
                    },
                )
                .collect::<Vec<_>>();
            let (spans_ptr, spans_len, spans_capacity) = leak_vec(spans);
            TermyFfiDamage {
                kind: 2,
                spans_ptr,
                spans_len,
                spans_capacity,
            }
        }
    };

    unsafe {
        *out_damage = result;
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_damage_free(damage: *mut TermyFfiDamage) -> TermyFfiStatus {
    if damage.is_null() {
        return TermyFfiStatus::Null;
    }

    let damage = unsafe { &mut *damage };
    if !damage.spans_ptr.is_null() {
        unsafe {
            drop(Vec::from_raw_parts(
                damage.spans_ptr,
                damage.spans_len,
                damage.spans_capacity,
            ));
        }
    }
    *damage = TermyFfiDamage::default();
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_drain_events(
    terminal: *mut TermyFfiTerminal,
    out_batch: *mut TermyFfiEventBatch,
) -> TermyFfiStatus {
    if terminal.is_null() || out_batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let (events, has_more) = unsafe { (*terminal).terminal.drain_events(&mut EmptyReplyHost) };
    let events = events
        .into_iter()
        .map(ffi_event_from_event)
        .collect::<Vec<_>>();
    let (events_ptr, events_len, events_capacity) = leak_vec(events);

    unsafe {
        *out_batch = TermyFfiEventBatch {
            events_ptr,
            events_len,
            events_capacity,
            has_more,
        };
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_event_batch_free(batch: *mut TermyFfiEventBatch) -> TermyFfiStatus {
    if batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let batch = unsafe { &mut *batch };
    if !batch.events_ptr.is_null() {
        let events = unsafe {
            Vec::from_raw_parts(batch.events_ptr, batch.events_len, batch.events_capacity)
        };
        for event in events {
            free_bytes(event.payload);
        }
    }
    *batch = TermyFfiEventBatch::default();
    TermyFfiStatus::Ok
}

fn free_bytes(bytes: TermyFfiBytes) {
    if bytes.ptr.is_null() {
        return;
    }

    unsafe {
        drop(Vec::from_raw_parts(bytes.ptr, bytes.len, bytes.capacity));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_buffer_free(bytes: TermyFfiBytes) -> TermyFfiStatus {
    if bytes.ptr.is_null() {
        return TermyFfiStatus::Null;
    }

    free_bytes(bytes);
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub extern "C" fn termy_null_buffer() -> TermyFfiBytes {
    TermyFfiBytes {
        ptr: ptr::null_mut(),
        len: 0,
        capacity: 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn termy_runtime_config_default_scrollback() -> usize {
    TerminalRuntimeConfig::default().scrollback_history
}

#[unsafe(no_mangle)]
pub extern "C" fn termy_terminal_options_default_scrollback() -> usize {
    TerminalOptions::default().scrollback_history
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_query_color_default_foreground(
    out_color: *mut TermyFfiColor,
) -> TermyFfiStatus {
    if out_color.is_null() {
        return TermyFfiStatus::Null;
    }

    let color = TerminalQueryColors::default().foreground;
    unsafe {
        *out_color = TermyFfiColor {
            r: color.r,
            g: color.g,
            b: color.b,
            a: 255,
        };
    }
    TermyFfiStatus::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_size_is_nonzero() {
        let size = termy_size_default();
        assert!(size.cols > 0);
        assert!(size.rows > 0);
        assert!(size.cell_width > 0.0);
        assert!(size.cell_height > 0.0);
    }
}
