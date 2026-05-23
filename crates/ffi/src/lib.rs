#![allow(clippy::missing_safety_doc)]

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    ptr, slice, str,
};

use termy_core::{
    ConfigDiagnostic, ConfigDiagnosticKind, LoadedTermyConfig, ProgressState, Terminal,
    TerminalClipboardTarget, TerminalDamageSnapshot, TerminalDirtySpan, TerminalEvent,
    TerminalKeyEventKind, TerminalMouseButton, TerminalMouseEventKind, TerminalMouseModifiers,
    TerminalMousePosition, TerminalOptions, TerminalQueryColors, TerminalReplyHost,
    TerminalRuntimeConfig, TerminalSize, TermyCell, TermyColor, TermyKeystroke, TermyModifiers,
    TermySearchMatch, TermySearchOptions, encode_mouse_report, keystroke_to_input,
    load_config_from_contents, load_config_from_default_path, load_config_from_path,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TermyFfiStatus {
    Ok = 0,
    Null = 1,
    InvalidUtf8 = 2,
    SpawnFailed = 3,
    ConfigLoadFailed = 4,
    UnknownKey = 5,
    WriteFailed = 6,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiSearchMatch {
    pub row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub line: TermyFfiBytes,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiSearchBatch {
    pub matches_ptr: *mut TermyFfiSearchMatch,
    pub matches_len: usize,
    pub matches_capacity: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiSearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiConfigDiagnostic {
    pub line_number: usize,
    pub kind: u32,
    pub message: TermyFfiBytes,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiConfigDiagnosticBatch {
    pub diagnostics_ptr: *mut TermyFfiConfigDiagnostic,
    pub diagnostics_len: usize,
    pub diagnostics_capacity: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TermyFfiRenderConfig {
    pub font_family: TermyFfiBytes,
    pub active_theme: TermyFfiBytes,
    pub foreground: TermyFfiColor,
    pub background: TermyFfiColor,
    pub cursor: TermyFfiColor,
    pub font_size: f32,
    pub line_height: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub background_opacity: f32,
    pub background_opacity_cells: bool,
    pub cursor_blink: bool,
    pub cursor_style: u32,
    pub cell_width: f32,
    pub cell_height: f32,
}

pub struct TermyFfiTerminal {
    terminal: Terminal,
}

pub struct TermyFfiConfig {
    loaded: LoadedTermyConfig,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiEnvVar {
    pub key_ptr: *const u8,
    pub key_len: usize,
    pub value_ptr: *const u8,
    pub value_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiTerminalOptions {
    pub config: *const TermyFfiConfig,
    pub working_directory_ptr: *const u8,
    pub working_directory_len: usize,
    pub startup_command_ptr: *const u8,
    pub startup_command_len: usize,
    pub env_vars_ptr: *const TermyFfiEnvVar,
    pub env_vars_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiKeystroke {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
    pub function: bool,
    pub key_ptr: *const u8,
    pub key_len: usize,
    pub key_char_ptr: *const u8,
    pub key_char_len: usize,
    pub event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TermyFfiMouseInput {
    pub kind: u32,
    pub button: u32,
    pub col: usize,
    pub row: usize,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
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

fn ffi_search_match_from_match(search_match: TermySearchMatch) -> TermyFfiSearchMatch {
    TermyFfiSearchMatch {
        row: search_match.row,
        start_col: search_match.start_col,
        end_col: search_match.end_col,
        line: ffi_bytes_from_string(search_match.line),
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

unsafe fn optional_utf8_owned(
    ptr: *const u8,
    len: usize,
) -> Result<Option<String>, TermyFfiStatus> {
    unsafe { optional_utf8(ptr, len) }.map(|value| value.map(ToOwned::to_owned))
}

unsafe fn required_utf8<'a>(ptr: *const u8, len: usize) -> Result<&'a str, TermyFfiStatus> {
    if ptr.is_null() {
        return Err(TermyFfiStatus::Null);
    }

    let bytes = unsafe { slice::from_raw_parts(ptr, len) };
    str::from_utf8(bytes).map_err(|_| TermyFfiStatus::InvalidUtf8)
}

unsafe fn contents_utf8<'a>(ptr: *const u8, len: usize) -> Result<&'a str, TermyFfiStatus> {
    if ptr.is_null() {
        if len == 0 {
            return Ok("");
        }
        return Err(TermyFfiStatus::Null);
    }

    unsafe { required_utf8(ptr, len) }
}

unsafe fn env_vars_from_ffi(
    ptr: *const TermyFfiEnvVar,
    len: usize,
) -> Result<HashMap<String, String>, TermyFfiStatus> {
    if len == 0 {
        return Ok(HashMap::new());
    }
    if ptr.is_null() {
        return Err(TermyFfiStatus::Null);
    }

    let env_vars = unsafe { slice::from_raw_parts(ptr, len) };
    let mut result = HashMap::with_capacity(env_vars.len());
    for env_var in env_vars {
        let key = unsafe { optional_utf8_owned(env_var.key_ptr, env_var.key_len) }?;
        let Some(key) = key.map(|value| value.trim().to_string()) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let value = unsafe { optional_utf8_owned(env_var.value_ptr, env_var.value_len) }?
            .unwrap_or_default();
        result.insert(key, value);
    }
    Ok(result)
}

fn config_diagnostic_kind(kind: ConfigDiagnosticKind) -> u32 {
    match kind {
        ConfigDiagnosticKind::UnknownSection => 1,
        ConfigDiagnosticKind::UnknownRootKey => 2,
        ConfigDiagnosticKind::UnknownColorKey => 3,
        ConfigDiagnosticKind::InvalidSyntax => 4,
        ConfigDiagnosticKind::InvalidValue => 5,
        ConfigDiagnosticKind::DuplicateRootKey => 6,
    }
}

fn mouse_button(button: u32) -> Option<TerminalMouseButton> {
    match button {
        1 => Some(TerminalMouseButton::Left),
        2 => Some(TerminalMouseButton::Middle),
        3 => Some(TerminalMouseButton::Right),
        _ => None,
    }
}

fn mouse_event(input: TermyFfiMouseInput) -> Option<TerminalMouseEventKind> {
    match input.kind {
        1 => Some(TerminalMouseEventKind::Press(mouse_button(input.button)?)),
        2 => Some(TerminalMouseEventKind::Release(mouse_button(input.button)?)),
        3 => Some(TerminalMouseEventKind::Drag(mouse_button(input.button)?)),
        4 => Some(TerminalMouseEventKind::Move),
        5 => Some(TerminalMouseEventKind::WheelUp),
        6 => Some(TerminalMouseEventKind::WheelDown),
        7 => Some(TerminalMouseEventKind::WheelLeft),
        8 => Some(TerminalMouseEventKind::WheelRight),
        _ => None,
    }
}

fn ffi_config_diagnostic_from_diagnostic(diagnostic: ConfigDiagnostic) -> TermyFfiConfigDiagnostic {
    TermyFfiConfigDiagnostic {
        line_number: diagnostic.line_number,
        kind: config_diagnostic_kind(diagnostic.kind),
        message: ffi_bytes_from_string(diagnostic.message),
    }
}

fn leak_loaded_config(
    loaded: Result<LoadedTermyConfig, termy_core::TermyConfigError>,
    out_config: *mut *mut TermyFfiConfig,
) -> TermyFfiStatus {
    if out_config.is_null() {
        return TermyFfiStatus::Null;
    }

    let Ok(loaded) = loaded else {
        return TermyFfiStatus::ConfigLoadFailed;
    };

    unsafe {
        *out_config = Box::into_raw(Box::new(TermyFfiConfig { loaded }));
    }
    TermyFfiStatus::Ok
}

unsafe fn terminal_new_with_runtime_config(
    size: TermyFfiSize,
    runtime_config: &TerminalRuntimeConfig,
    configured_working_dir: Option<&str>,
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
        configured_working_dir,
        None,
        None,
        Some(runtime_config),
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
    unsafe {
        terminal_new_with_runtime_config(
            size,
            &TerminalRuntimeConfig::default(),
            None,
            startup_command_ptr,
            startup_command_len,
            out_terminal,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_new_with_config(
    size: TermyFfiSize,
    config: *const TermyFfiConfig,
    startup_command_ptr: *const u8,
    startup_command_len: usize,
    out_terminal: *mut *mut TermyFfiTerminal,
) -> TermyFfiStatus {
    if config.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        terminal_new_with_runtime_config(
            size,
            &(*config).loaded.runtime_config,
            None,
            startup_command_ptr,
            startup_command_len,
            out_terminal,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_new_with_options(
    size: TermyFfiSize,
    options: *const TermyFfiTerminalOptions,
    out_terminal: *mut *mut TermyFfiTerminal,
) -> TermyFfiStatus {
    if options.is_null() || out_terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    let options = unsafe { *options };
    let mut runtime_config = if options.config.is_null() {
        TerminalRuntimeConfig::default()
    } else {
        unsafe { (*options.config).loaded.runtime_config.clone() }
    };
    let working_directory = match unsafe {
        optional_utf8(options.working_directory_ptr, options.working_directory_len)
    } {
        Ok(value) => value,
        Err(status) => return status,
    };
    let environment = match unsafe { env_vars_from_ffi(options.env_vars_ptr, options.env_vars_len) }
    {
        Ok(value) => value,
        Err(status) => return status,
    };
    runtime_config.environment.extend(environment);

    unsafe {
        terminal_new_with_runtime_config(
            size,
            &runtime_config,
            working_directory,
            options.startup_command_ptr,
            options.startup_command_len,
            out_terminal,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_load_default(
    out_config: *mut *mut TermyFfiConfig,
) -> TermyFfiStatus {
    leak_loaded_config(load_config_from_default_path(), out_config)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_load_path(
    path_ptr: *const u8,
    path_len: usize,
    out_config: *mut *mut TermyFfiConfig,
) -> TermyFfiStatus {
    let path = match unsafe { required_utf8(path_ptr, path_len) } {
        Ok(path) => path,
        Err(status) => return status,
    };
    leak_loaded_config(load_config_from_path(Path::new(path)), out_config)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_from_contents(
    contents_ptr: *const u8,
    contents_len: usize,
    out_config: *mut *mut TermyFfiConfig,
) -> TermyFfiStatus {
    if out_config.is_null() {
        return TermyFfiStatus::Null;
    }

    let contents = match unsafe { contents_utf8(contents_ptr, contents_len) } {
        Ok(contents) => contents,
        Err(status) => return status,
    };

    let loaded = load_config_from_contents(contents);
    unsafe {
        *out_config = Box::into_raw(Box::new(TermyFfiConfig { loaded }));
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_free(config: *mut TermyFfiConfig) -> TermyFfiStatus {
    if config.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        drop(Box::from_raw(config));
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_loaded_from_disk(config: *const TermyFfiConfig) -> bool {
    if config.is_null() {
        return false;
    }

    unsafe { (*config).loaded.loaded_from_disk }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_runtime_scrollback_history(
    config: *const TermyFfiConfig,
) -> usize {
    if config.is_null() {
        return 0;
    }

    unsafe { (*config).loaded.runtime_config.scrollback_history }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_diagnostic_count(config: *const TermyFfiConfig) -> usize {
    if config.is_null() {
        return 0;
    }

    unsafe { (*config).loaded.diagnostics.len() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_window_size(
    config: *const TermyFfiConfig,
    out_width: *mut f32,
    out_height: *mut f32,
) -> TermyFfiStatus {
    if config.is_null() || out_width.is_null() || out_height.is_null() {
        return TermyFfiStatus::Null;
    }

    let app_config = unsafe { &(*config).loaded.app_config };
    unsafe {
        *out_width = app_config.window_width;
        *out_height = app_config.window_height;
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_working_directory(
    config: *const TermyFfiConfig,
    out_working_directory: *mut TermyFfiBytes,
) -> TermyFfiStatus {
    if config.is_null() || out_working_directory.is_null() {
        return TermyFfiStatus::Null;
    }

    let working_directory = unsafe { (*config).loaded.app_config.working_dir.as_ref() };
    let bytes = working_directory.map_or_else(
        || termy_null_buffer(),
        |working_directory| ffi_bytes_from_string(working_directory.clone()),
    );
    unsafe {
        *out_working_directory = bytes;
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_path(
    config: *const TermyFfiConfig,
    out_path: *mut TermyFfiBytes,
) -> TermyFfiStatus {
    if config.is_null() || out_path.is_null() {
        return TermyFfiStatus::Null;
    }

    let path = unsafe { (*config).loaded.path.as_ref() };
    let bytes = path.map_or_else(
        || termy_null_buffer(),
        |path| ffi_bytes_from_string(path.to_string_lossy().into_owned()),
    );
    unsafe {
        *out_path = bytes;
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_diagnostics(
    config: *const TermyFfiConfig,
    out_batch: *mut TermyFfiConfigDiagnosticBatch,
) -> TermyFfiStatus {
    if config.is_null() || out_batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let diagnostics = unsafe {
        (*config)
            .loaded
            .diagnostics
            .clone()
            .into_iter()
            .map(ffi_config_diagnostic_from_diagnostic)
            .collect::<Vec<_>>()
    };
    let (diagnostics_ptr, diagnostics_len, diagnostics_capacity) = leak_vec(diagnostics);

    unsafe {
        *out_batch = TermyFfiConfigDiagnosticBatch {
            diagnostics_ptr,
            diagnostics_len,
            diagnostics_capacity,
        };
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_diagnostics_free(
    batch: *mut TermyFfiConfigDiagnosticBatch,
) -> TermyFfiStatus {
    if batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let batch = unsafe { &mut *batch };
    if !batch.diagnostics_ptr.is_null() {
        let diagnostics = unsafe {
            Vec::from_raw_parts(
                batch.diagnostics_ptr,
                batch.diagnostics_len,
                batch.diagnostics_capacity,
            )
        };
        for diagnostic in diagnostics {
            free_bytes(diagnostic.message);
        }
    }
    *batch = TermyFfiConfigDiagnosticBatch::default();
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_config_render_config(
    config: *const TermyFfiConfig,
    out_render_config: *mut TermyFfiRenderConfig,
) -> TermyFfiStatus {
    if config.is_null() || out_render_config.is_null() {
        return TermyFfiStatus::Null;
    }

    let loaded = unsafe { &(*config).loaded };
    let app_config = &loaded.app_config;
    let cell_metrics = termy_core::measure_cell_from_config(app_config);
    let theme_colors = termy_core::resolve_theme_colors_from_app_config(
        app_config,
        loaded.path.as_deref(),
        termy_core::SystemAppearance::Dark,
    );
    unsafe {
        *out_render_config = TermyFfiRenderConfig {
            font_family: ffi_bytes_from_string(app_config.font_family.clone()),
            active_theme: ffi_bytes_from_string(theme_colors.active_theme),
            foreground: theme_colors.foreground.into(),
            background: theme_colors.background.into(),
            cursor: theme_colors.cursor.into(),
            font_size: app_config.font_size,
            line_height: app_config.line_height,
            padding_x: app_config.padding_x,
            padding_y: app_config.padding_y,
            background_opacity: app_config.background_opacity,
            background_opacity_cells: app_config.background_opacity_cells,
            cursor_blink: app_config.cursor_blink,
            cursor_style: match app_config.cursor_style {
                termy_core::AppConfigCursorStyle::Line => 1,
                termy_core::AppConfigCursorStyle::Block => 2,
            },
            cell_width: cell_metrics.cell_width,
            cell_height: cell_metrics.cell_height,
        };
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_render_config_free(
    render_config: *mut TermyFfiRenderConfig,
) -> TermyFfiStatus {
    if render_config.is_null() {
        return TermyFfiStatus::Null;
    }

    let render_config = unsafe { &mut *render_config };
    free_bytes(render_config.font_family);
    free_bytes(render_config.active_theme);
    *render_config = TermyFfiRenderConfig::default();
    TermyFfiStatus::Ok
}

// ---------------------------------------------------------------------------
// Settings (native settings window <-> config.txt bridge)
// ---------------------------------------------------------------------------

use termy_config_core as cfg;

fn settings_read_contents() -> String {
    match cfg::config_path().and_then(|path| std::fs::read_to_string(path).ok()) {
        Some(contents) if !contents.trim().is_empty() => contents,
        _ => cfg::DEFAULT_CONFIG_TEMPLATE.to_string(),
    }
}

fn settings_write_contents(contents: &str) -> Result<(), TermyFfiStatus> {
    let path = cfg::config_path().ok_or(TermyFfiStatus::WriteFailed)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| TermyFfiStatus::WriteFailed)?;
    }
    std::fs::write(&path, contents).map_err(|_| TermyFfiStatus::WriteFailed)
}

fn settings_color_hex(app: &cfg::AppConfig, id: cfg::ColorSettingId) -> Option<String> {
    use cfg::ColorSettingId::*;
    let rgb = match id {
        Foreground => app.colors.foreground,
        Background => app.colors.background,
        Cursor => app.colors.cursor,
        Black => app.colors.ansi[0],
        Red => app.colors.ansi[1],
        Green => app.colors.ansi[2],
        Yellow => app.colors.ansi[3],
        Blue => app.colors.ansi[4],
        Magenta => app.colors.ansi[5],
        Cyan => app.colors.ansi[6],
        White => app.colors.ansi[7],
        BrightBlack => app.colors.ansi[8],
        BrightRed => app.colors.ansi[9],
        BrightGreen => app.colors.ansi[10],
        BrightYellow => app.colors.ansi[11],
        BrightBlue => app.colors.ansi[12],
        BrightMagenta => app.colors.ansi[13],
        BrightCyan => app.colors.ansi[14],
        BrightWhite => app.colors.ansi[15],
    };
    rgb.map(|c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
}

const SETTINGS_BUILTIN_THEME_IDS: &[&str] = &[
    "termy",
    "tokyo-night",
    "catppuccin-mocha",
    "dracula",
    "gruvbox-dark",
    "nord",
    "solarized-dark",
    "one-dark",
    "monokai",
    "material-dark",
    "palenight",
    "tomorrow-night",
    "oceanic-next",
];

fn settings_normalize_theme_id(theme_id: &str) -> String {
    let mut normalized = String::new();
    let mut last_dash = false;

    for character in theme_id.trim().chars() {
        let character = character.to_ascii_lowercase();
        match character {
            'a'..='z' | '0'..='9' => {
                normalized.push(character);
                last_dash = false;
            }
            '-' | '_' | ' ' if !normalized.is_empty() && !last_dash => {
                normalized.push('-');
                last_dash = true;
            }
            _ => {}
        }
    }

    while normalized.ends_with('-') {
        normalized.pop();
    }

    normalized
}

fn settings_installed_theme_ids(config_path: Option<&Path>) -> Vec<String> {
    let owned_config_path;
    let config_path = if let Some(path) = config_path {
        path
    } else {
        owned_config_path = match cfg::config_path() {
            Some(path) => path,
            None => return Vec::new(),
        };
        owned_config_path.as_path()
    };
    let Some(config_dir) = config_path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(config_dir.join("themes")) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
            if !is_json {
                return None;
            }

            let stem = path.file_stem()?.to_str()?;
            let normalized = settings_normalize_theme_id(stem);
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect()
}

fn settings_theme_label(theme_id: &str) -> String {
    if theme_id == cfg::SHELL_DECIDE_THEME_ID {
        return "Shell Decide".to_string();
    }

    theme_id
        .split(|character| character == '-' || character == '_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn settings_theme_choices(
    loaded: &LoadedTermyConfig,
    current_value: Option<&str>,
) -> Vec<serde_json::Value> {
    use serde_json::json;

    let mut ids = BTreeSet::new();
    ids.insert(cfg::SHELL_DECIDE_THEME_ID.to_string());
    ids.extend(
        SETTINGS_BUILTIN_THEME_IDS
            .iter()
            .map(|theme_id| (*theme_id).to_string()),
    );
    ids.extend(settings_installed_theme_ids(loaded.path.as_deref()));
    if let Some(current_value) = current_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        ids.insert(current_value.to_string());
    }

    ids.into_iter()
        .map(|theme_id| {
            json!({
                "value": theme_id,
                "label": settings_theme_label(&theme_id),
            })
        })
        .collect()
}

fn settings_schema_json(loaded: &LoadedTermyConfig) -> String {
    use serde_json::{Value, json};

    let app = &loaded.app_config;
    let sections_meta = [
        (
            cfg::SettingsSection::Appearance,
            "appearance",
            "Appearance",
            "paintbrush",
        ),
        (
            cfg::SettingsSection::Terminal,
            "terminal",
            "Terminal",
            "terminal",
        ),
        (
            cfg::SettingsSection::Tabs,
            "tabs",
            "Tabs",
            "square.on.square",
        ),
        (
            cfg::SettingsSection::Advanced,
            "advanced",
            "Advanced",
            "gearshape.2",
        ),
        (
            cfg::SettingsSection::Colors,
            "colors",
            "Colors",
            "paintpalette",
        ),
        (
            cfg::SettingsSection::Keybindings,
            "keybindings",
            "Keybindings",
            "keyboard",
        ),
    ];

    let mut sections = Vec::new();
    for (section, id, label, icon) in sections_meta {
        let mut obj = json!({ "id": id, "label": label, "systemImage": icon });

        match section {
            cfg::SettingsSection::Colors => {
                let colors: Vec<Value> = cfg::COLOR_SETTING_SPECS
                    .iter()
                    .map(|spec| {
                        json!({
                            "key": spec.key,
                            "title": spec.title,
                            "description": spec.description,
                            "hex": settings_color_hex(app, spec.id),
                        })
                    })
                    .collect();
                obj["colors"] = json!(colors);
            }
            cfg::SettingsSection::Keybindings => {
                let lines: Vec<&str> = app
                    .keybind_lines
                    .iter()
                    .map(|line| line.value.as_str())
                    .collect();
                obj["keybinds"] = json!(lines);
            }
            _ => {
                let mut groups: Vec<(&str, Vec<Value>)> = Vec::new();
                for spec in cfg::ROOT_SETTING_SPECS {
                    if spec.section != section || matches!(spec.id, cfg::RootSettingId::Keybind) {
                        continue;
                    }

                    let kind = match spec.value_kind {
                        cfg::RootSettingValueKind::Text => "text",
                        cfg::RootSettingValueKind::Numeric => "numeric",
                        cfg::RootSettingValueKind::Boolean => "boolean",
                        cfg::RootSettingValueKind::Enum => "enum",
                        cfg::RootSettingValueKind::Special => "special",
                    };

                    let mut setting = json!({
                        "key": spec.key,
                        "title": spec.title,
                        "description": spec.description,
                        "kind": kind,
                        "value": cfg::root_setting_default_value(app, spec.id),
                    });

                    if let Some(choices) = cfg::root_setting_enum_choices(spec.id) {
                        let choices: Vec<Value> = choices
                            .iter()
                            .map(|choice| json!({ "value": choice.value, "label": choice.label }))
                            .collect();
                        setting["choices"] = json!(choices);
                    }
                    if matches!(
                        spec.id,
                        cfg::RootSettingId::Theme
                            | cfg::RootSettingId::ThemeLight
                            | cfg::RootSettingId::ThemeDark
                    ) {
                        setting["choices"] =
                            json!(settings_theme_choices(loaded, setting["value"].as_str()));
                    }

                    match groups.iter_mut().find(|(group, _)| *group == spec.group) {
                        Some((_, settings)) => settings.push(setting),
                        None => groups.push((spec.group, vec![setting])),
                    }
                }

                let groups: Vec<Value> = groups
                    .into_iter()
                    .map(|(group, settings)| json!({ "label": group, "settings": settings }))
                    .collect();
                obj["groups"] = json!(groups);
            }
        }

        sections.push(obj);
    }

    json!({
        "configPath": loaded.path.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "loadedFromDisk": loaded.loaded_from_disk,
        "sections": sections,
    })
    .to_string()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_settings_schema_json(
    config: *const TermyFfiConfig,
    out_bytes: *mut TermyFfiBytes,
) -> TermyFfiStatus {
    if config.is_null() || out_bytes.is_null() {
        return TermyFfiStatus::Null;
    }

    let loaded = unsafe { &(*config).loaded };
    let json = settings_schema_json(loaded);
    unsafe {
        *out_bytes = ffi_bytes_from_string(json);
    }
    TermyFfiStatus::Ok
}

unsafe fn settings_set_root_inner(
    key_ptr: *const u8,
    key_len: usize,
    value_ptr: *const u8,
    value_len: usize,
) -> Result<(), TermyFfiStatus> {
    let key = unsafe { required_utf8(key_ptr, key_len) }?;
    let value = unsafe { optional_utf8(value_ptr, value_len) }?.unwrap_or("");
    let id = cfg::root_setting_from_key(key).ok_or(TermyFfiStatus::UnknownKey)?;
    let updated = cfg::upsert_root_setting(&settings_read_contents(), id, value.trim());
    settings_write_contents(&updated)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_settings_set_root(
    key_ptr: *const u8,
    key_len: usize,
    value_ptr: *const u8,
    value_len: usize,
) -> TermyFfiStatus {
    match unsafe { settings_set_root_inner(key_ptr, key_len, value_ptr, value_len) } {
        Ok(()) => TermyFfiStatus::Ok,
        Err(status) => status,
    }
}

unsafe fn settings_reset_root_inner(
    key_ptr: *const u8,
    key_len: usize,
) -> Result<(), TermyFfiStatus> {
    let key = unsafe { required_utf8(key_ptr, key_len) }?;
    let id = cfg::root_setting_from_key(key).ok_or(TermyFfiStatus::UnknownKey)?;
    let updated = cfg::remove_root_setting(&settings_read_contents(), id);
    settings_write_contents(&updated)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_settings_reset_root(
    key_ptr: *const u8,
    key_len: usize,
) -> TermyFfiStatus {
    match unsafe { settings_reset_root_inner(key_ptr, key_len) } {
        Ok(()) => TermyFfiStatus::Ok,
        Err(status) => status,
    }
}

unsafe fn settings_set_color_inner(
    key_ptr: *const u8,
    key_len: usize,
    hex_ptr: *const u8,
    hex_len: usize,
) -> Result<(), TermyFfiStatus> {
    let key = unsafe { required_utf8(key_ptr, key_len) }?;
    let id = cfg::color_setting_from_key(key).ok_or(TermyFfiStatus::UnknownKey)?;
    let value = unsafe { optional_utf8(hex_ptr, hex_len) }?.map(|hex| hex.trim().to_string());
    let updated = cfg::apply_color_updates(
        &settings_read_contents(),
        &[cfg::ColorSettingUpdate { id, value }],
    );
    settings_write_contents(&updated)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_settings_set_color(
    key_ptr: *const u8,
    key_len: usize,
    hex_ptr: *const u8,
    hex_len: usize,
) -> TermyFfiStatus {
    match unsafe { settings_set_color_inner(key_ptr, key_len, hex_ptr, hex_len) } {
        Ok(()) => TermyFfiStatus::Ok,
        Err(status) => status,
    }
}

unsafe fn settings_set_keybinds_inner(
    text_ptr: *const u8,
    text_len: usize,
) -> Result<(), TermyFfiStatus> {
    let text = unsafe { optional_utf8(text_ptr, text_len) }?.unwrap_or("");
    let lines: Vec<String> = text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    let updated = cfg::replace_keybind_lines(&settings_read_contents(), &lines);
    settings_write_contents(&updated)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_settings_set_keybinds(
    text_ptr: *const u8,
    text_len: usize,
) -> TermyFfiStatus {
    match unsafe { settings_set_keybinds_inner(text_ptr, text_len) } {
        Ok(()) => TermyFfiStatus::Ok,
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_reload_default_config_colors(
    terminal: *mut TermyFfiTerminal,
) -> TermyFfiStatus {
    if terminal.is_null() {
        return TermyFfiStatus::Null;
    }

    let Ok(loaded) = load_config_from_default_path() else {
        return TermyFfiStatus::ConfigLoadFailed;
    };
    let query_colors = termy_core::terminal_query_colors_from_resolved_theme(
        &termy_core::resolve_theme_colors_from_app_config(
            &loaded.app_config,
            loaded.path.as_deref(),
            termy_core::SystemAppearance::Dark,
        ),
    );
    unsafe {
        (*terminal).terminal.set_query_colors(query_colors);
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
pub unsafe extern "C" fn termy_terminal_encode_key(
    terminal: *mut TermyFfiTerminal,
    keystroke: *const TermyFfiKeystroke,
    out_bytes: *mut TermyFfiBytes,
) -> TermyFfiStatus {
    if terminal.is_null() || keystroke.is_null() || out_bytes.is_null() {
        return TermyFfiStatus::Null;
    }

    let keystroke = unsafe { *keystroke };
    let key = match unsafe { required_utf8(keystroke.key_ptr, keystroke.key_len) } {
        Ok(key) => key.to_owned(),
        Err(status) => return status,
    };
    let key_char = match unsafe { optional_utf8(keystroke.key_char_ptr, keystroke.key_char_len) } {
        Ok(key_char) => key_char.map(ToOwned::to_owned),
        Err(status) => return status,
    };
    let event_kind = match keystroke.event_kind {
        2 => TerminalKeyEventKind::Repeat,
        3 => TerminalKeyEventKind::Release,
        _ => TerminalKeyEventKind::Press,
    };
    let input = unsafe {
        let terminal = &(*terminal).terminal;
        keystroke_to_input(
            &TermyKeystroke {
                modifiers: TermyModifiers {
                    control: keystroke.control,
                    alt: keystroke.alt,
                    shift: keystroke.shift,
                    platform: keystroke.platform,
                    function: keystroke.function,
                },
                key,
                key_char,
            },
            event_kind,
            terminal.keyboard_mode(),
            true,
        )
    };

    unsafe {
        *out_bytes = input.map_or_else(|| termy_null_buffer(), ffi_bytes_from_vec);
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_encode_mouse(
    terminal: *mut TermyFfiTerminal,
    input: *const TermyFfiMouseInput,
    out_bytes: *mut TermyFfiBytes,
) -> TermyFfiStatus {
    if terminal.is_null() || input.is_null() || out_bytes.is_null() {
        return TermyFfiStatus::Null;
    }

    let input = unsafe { *input };
    let Some(event) = mouse_event(input) else {
        return TermyFfiStatus::UnknownKey;
    };

    let encoded = unsafe {
        let terminal = &(*terminal).terminal;
        encode_mouse_report(
            terminal.mouse_mode(),
            event,
            TerminalMousePosition {
                col: input.col,
                row: input.row,
            },
            TerminalMouseModifiers {
                shift: input.shift,
                alt: input.alt,
                control: input.control,
            },
        )
    };

    unsafe {
        *out_bytes = encoded.map_or_else(|| termy_null_buffer(), ffi_bytes_from_vec);
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
pub unsafe extern "C" fn termy_terminal_scroll_display(
    terminal: *mut TermyFfiTerminal,
    delta_lines: i32,
    out_changed: *mut bool,
) -> TermyFfiStatus {
    if terminal.is_null() || out_changed.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        *out_changed = (*terminal).terminal.scroll_display(delta_lines);
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_scroll_to_bottom(
    terminal: *mut TermyFfiTerminal,
    out_changed: *mut bool,
) -> TermyFfiStatus {
    if terminal.is_null() || out_changed.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        *out_changed = (*terminal).terminal.scroll_to_bottom();
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_clear_scrollback(
    terminal: *mut TermyFfiTerminal,
    out_changed: *mut bool,
) -> TermyFfiStatus {
    if terminal.is_null() || out_changed.is_null() {
        return TermyFfiStatus::Null;
    }

    unsafe {
        *out_changed = (*terminal).terminal.clear_scrollback();
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_search(
    terminal: *mut TermyFfiTerminal,
    query_ptr: *const u8,
    query_len: usize,
    out_batch: *mut TermyFfiSearchBatch,
) -> TermyFfiStatus {
    unsafe {
        termy_terminal_search_with_options(
            terminal,
            query_ptr,
            query_len,
            TermyFfiSearchOptions::default(),
            out_batch,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_terminal_search_with_options(
    terminal: *mut TermyFfiTerminal,
    query_ptr: *const u8,
    query_len: usize,
    options: TermyFfiSearchOptions,
    out_batch: *mut TermyFfiSearchBatch,
) -> TermyFfiStatus {
    if terminal.is_null() || out_batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let query = match unsafe { contents_utf8(query_ptr, query_len) } {
        Ok(query) => query,
        Err(status) => return status,
    };

    let matches = unsafe {
        (*terminal).terminal.search_with_options(
            query,
            TermySearchOptions {
                case_sensitive: options.case_sensitive,
                regex: options.regex,
            },
        )
    }
    .into_iter()
    .map(ffi_search_match_from_match)
    .collect::<Vec<_>>();
    let (matches_ptr, matches_len, matches_capacity) = leak_vec(matches);

    unsafe {
        *out_batch = TermyFfiSearchBatch {
            matches_ptr,
            matches_len,
            matches_capacity,
        };
    }
    TermyFfiStatus::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn termy_search_batch_free(
    batch: *mut TermyFfiSearchBatch,
) -> TermyFfiStatus {
    if batch.is_null() {
        return TermyFfiStatus::Null;
    }

    let batch = unsafe { &mut *batch };
    if !batch.matches_ptr.is_null() {
        let matches = unsafe {
            Vec::from_raw_parts(batch.matches_ptr, batch.matches_len, batch.matches_capacity)
        };
        for search_match in matches {
            free_bytes(search_match.line);
        }
    }
    *batch = TermyFfiSearchBatch::default();
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

    #[test]
    fn settings_schema_json_covers_sections_and_values() {
        let contents = b"font_size = 18\ncursor_style = line\n[colors]\nforeground = #abcdef\n";
        let mut config = ptr::null_mut();
        assert_eq!(
            unsafe { termy_config_from_contents(contents.as_ptr(), contents.len(), &mut config) },
            TermyFfiStatus::Ok
        );

        let mut bytes = TermyFfiBytes::default();
        assert_eq!(
            unsafe { termy_settings_schema_json(config, &mut bytes) },
            TermyFfiStatus::Ok
        );
        let json = unsafe {
            str::from_utf8(slice::from_raw_parts(bytes.ptr, bytes.len)).expect("schema utf8")
        };
        let value: serde_json::Value = serde_json::from_str(json).expect("valid json");

        let sections = value["sections"].as_array().expect("sections array");
        let ids: Vec<&str> = sections
            .iter()
            .map(|section| section["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![
                "appearance",
                "terminal",
                "tabs",
                "advanced",
                "colors",
                "keybindings"
            ]
        );

        // Appearance carries the edited font size value.
        let appearance = &sections[0];
        let font_size = appearance["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["settings"].as_array().unwrap())
            .find(|setting| setting["key"] == "font_size")
            .expect("font_size setting");
        assert_eq!(font_size["value"], "18");
        assert_eq!(font_size["kind"], "numeric");
        let theme = appearance["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["settings"].as_array().unwrap())
            .find(|setting| setting["key"] == "theme")
            .expect("theme setting");
        assert_eq!(theme["kind"], "special");
        let theme_choices = theme["choices"].as_array().expect("theme choices");
        assert!(
            theme_choices
                .iter()
                .any(|choice| choice["value"] == cfg::SHELL_DECIDE_THEME_ID)
        );
        assert!(
            theme_choices
                .iter()
                .any(|choice| choice["value"] == "tokyo-night")
        );

        // Colors section reflects the override hex.
        let colors = &sections[4]["colors"].as_array().unwrap();
        let foreground = colors
            .iter()
            .find(|color| color["key"] == "foreground")
            .expect("foreground color");
        assert_eq!(foreground["hex"], "#abcdef");

        assert_eq!(unsafe { termy_buffer_free(bytes) }, TermyFfiStatus::Ok);
        assert_eq!(unsafe { termy_config_free(config) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn config_from_contents_exposes_runtime_fields_and_diagnostics() {
        let contents = b"scrollback = 77\nwindow_width = 1440\nwindow_height = 900\nworking_dir = /tmp\nunknown_key = true\n";
        let mut config = ptr::null_mut();

        let status =
            unsafe { termy_config_from_contents(contents.as_ptr(), contents.len(), &mut config) };
        assert_eq!(status, TermyFfiStatus::Ok);
        assert!(!config.is_null());

        assert_eq!(
            unsafe { termy_config_runtime_scrollback_history(config) },
            77
        );
        assert_eq!(unsafe { termy_config_diagnostic_count(config) }, 1);

        let mut width = 0.0;
        let mut height = 0.0;
        assert_eq!(
            unsafe { termy_config_window_size(config, &mut width, &mut height) },
            TermyFfiStatus::Ok
        );
        assert_eq!(width, 1440.0);
        assert_eq!(height, 900.0);

        let mut working_directory = TermyFfiBytes::default();
        assert_eq!(
            unsafe { termy_config_working_directory(config, &mut working_directory) },
            TermyFfiStatus::Ok
        );
        let working_directory_text = unsafe {
            str::from_utf8(slice::from_raw_parts(
                working_directory.ptr,
                working_directory.len,
            ))
            .expect("working directory utf8")
        };
        assert_eq!(working_directory_text, "/tmp");
        assert_eq!(
            unsafe { termy_buffer_free(working_directory) },
            TermyFfiStatus::Ok
        );

        let mut diagnostics = TermyFfiConfigDiagnosticBatch::default();
        assert_eq!(
            unsafe { termy_config_diagnostics(config, &mut diagnostics) },
            TermyFfiStatus::Ok
        );
        assert_eq!(diagnostics.diagnostics_len, 1);
        let first = unsafe { *diagnostics.diagnostics_ptr };
        assert_eq!(first.line_number, 5);
        assert_eq!(first.kind, 2);
        assert!(!first.message.ptr.is_null());

        assert_eq!(
            unsafe { termy_config_diagnostics_free(&mut diagnostics) },
            TermyFfiStatus::Ok
        );
        assert_eq!(unsafe { termy_config_free(config) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn config_from_contents_exposes_render_config() {
        let contents = b"theme = nord\nfont_family = Example Mono\nfont_size = 18\nline_height = 1.25\npadding_x = 3\npadding_y = 5\nbackground_opacity = 0.5\nbackground_opacity_cells = true\ncursor_blink = false\ncursor_style = line\n[colors]\nbackground = #010203\ncursor = #040506\n";
        let mut config = ptr::null_mut();

        let status =
            unsafe { termy_config_from_contents(contents.as_ptr(), contents.len(), &mut config) };
        assert_eq!(status, TermyFfiStatus::Ok);
        assert!(!config.is_null());

        let mut render_config = TermyFfiRenderConfig::default();
        assert_eq!(
            unsafe { termy_config_render_config(config, &mut render_config) },
            TermyFfiStatus::Ok
        );
        let font_family = unsafe {
            str::from_utf8(slice::from_raw_parts(
                render_config.font_family.ptr,
                render_config.font_family.len,
            ))
            .expect("font family utf8")
        };
        let active_theme = unsafe {
            str::from_utf8(slice::from_raw_parts(
                render_config.active_theme.ptr,
                render_config.active_theme.len,
            ))
            .expect("active theme utf8")
        };

        assert_eq!(font_family, "Example Mono");
        assert_eq!(active_theme, "nord");
        assert_eq!(
            render_config.background,
            TermyFfiColor {
                r: 1,
                g: 2,
                b: 3,
                a: 255,
            }
        );
        assert_eq!(
            render_config.cursor,
            TermyFfiColor {
                r: 4,
                g: 5,
                b: 6,
                a: 255,
            }
        );
        assert_eq!(render_config.font_size, 18.0);
        assert_eq!(render_config.line_height, 1.25);
        assert_eq!(render_config.padding_x, 3.0);
        assert_eq!(render_config.padding_y, 5.0);
        assert_eq!(render_config.background_opacity, 0.5);
        assert!(render_config.background_opacity_cells);
        assert!(!render_config.cursor_blink);
        assert_eq!(render_config.cursor_style, 1);
        assert!(render_config.cell_width >= 1.0);
        assert_eq!(render_config.cell_height, 22.5);

        assert_eq!(
            unsafe { termy_render_config_free(&mut render_config) },
            TermyFfiStatus::Ok
        );
        assert_eq!(unsafe { termy_config_free(config) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn terminal_search_returns_visible_matches() {
        let size = TermyFfiSize {
            cols: 16,
            rows: 4,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        #[cfg(target_os = "windows")]
        let command: &[u8] = b"echo alpha beta && echo beta gamma";
        #[cfg(not(target_os = "windows"))]
        let command: &[u8] = b"printf 'alpha beta\nbeta gamma'";
        let mut terminal = ptr::null_mut();

        assert_eq!(
            unsafe { termy_terminal_new(size, command.as_ptr(), command.len(), &mut terminal,) },
            TermyFfiStatus::Ok
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        let query = b"beta";
        let mut batch = TermyFfiSearchBatch::default();
        assert_eq!(
            unsafe { termy_terminal_search(terminal, query.as_ptr(), query.len(), &mut batch) },
            TermyFfiStatus::Ok
        );
        assert!(batch.matches_len >= 1);

        let matches = unsafe { slice::from_raw_parts(batch.matches_ptr, batch.matches_len) };
        assert!(
            matches
                .iter()
                .any(|search_match| search_match.start_col == 6)
        );

        assert_eq!(
            unsafe { termy_search_batch_free(&mut batch) },
            TermyFfiStatus::Ok
        );
        assert_eq!(unsafe { termy_terminal_free(terminal) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn terminal_search_with_options_supports_case_sensitive_matching() {
        let size = TermyFfiSize {
            cols: 16,
            rows: 4,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        #[cfg(target_os = "windows")]
        let command: &[u8] = b"echo alpha Beta && echo beta gamma";
        #[cfg(not(target_os = "windows"))]
        let command: &[u8] = b"printf 'alpha Beta\nbeta gamma'";
        let mut terminal = ptr::null_mut();

        assert_eq!(
            unsafe { termy_terminal_new(size, command.as_ptr(), command.len(), &mut terminal,) },
            TermyFfiStatus::Ok
        );
        std::thread::sleep(std::time::Duration::from_millis(100));

        let query = b"beta";
        let mut batch = TermyFfiSearchBatch::default();
        assert_eq!(
            unsafe {
                termy_terminal_search_with_options(
                    terminal,
                    query.as_ptr(),
                    query.len(),
                    TermyFfiSearchOptions {
                        case_sensitive: true,
                        regex: false,
                    },
                    &mut batch,
                )
            },
            TermyFfiStatus::Ok
        );

        let matches = unsafe { slice::from_raw_parts(batch.matches_ptr, batch.matches_len) };
        assert!(!matches.is_empty());
        assert!(
            matches
                .iter()
                .all(|search_match| search_match.start_col == 0)
        );

        assert_eq!(
            unsafe { termy_search_batch_free(&mut batch) },
            TermyFfiStatus::Ok
        );
        assert_eq!(unsafe { termy_terminal_free(terminal) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn terminal_encode_key_uses_core_keyboard_mapping() {
        let size = TermyFfiSize {
            cols: 16,
            rows: 4,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut terminal = ptr::null_mut();

        assert_eq!(
            unsafe { termy_terminal_new(size, ptr::null(), 0, &mut terminal) },
            TermyFfiStatus::Ok
        );

        let key = b"tab";
        let keystroke = TermyFfiKeystroke {
            shift: true,
            key_ptr: key.as_ptr(),
            key_len: key.len(),
            event_kind: 1,
            ..TermyFfiKeystroke::default()
        };
        let mut bytes = TermyFfiBytes::default();
        assert_eq!(
            unsafe { termy_terminal_encode_key(terminal, &keystroke, &mut bytes) },
            TermyFfiStatus::Ok
        );
        let encoded = unsafe { slice::from_raw_parts(bytes.ptr, bytes.len) };
        assert_eq!(encoded, b"\x1b[Z");

        assert_eq!(unsafe { termy_buffer_free(bytes) }, TermyFfiStatus::Ok);
        assert_eq!(unsafe { termy_terminal_free(terminal) }, TermyFfiStatus::Ok);
    }

    #[test]
    fn terminal_encode_mouse_uses_live_mouse_mode() {
        let size = TermyFfiSize {
            cols: 16,
            rows: 4,
            cell_width: 9.0,
            cell_height: 18.0,
        };
        let mut terminal = ptr::null_mut();

        assert_eq!(
            unsafe { termy_terminal_new(size, ptr::null(), 0, &mut terminal) },
            TermyFfiStatus::Ok
        );

        let input = TermyFfiMouseInput {
            kind: 1,
            button: 1,
            col: 4,
            row: 2,
            ..TermyFfiMouseInput::default()
        };
        let mut bytes = TermyFfiBytes::default();
        assert_eq!(
            unsafe { termy_terminal_encode_mouse(terminal, &input, &mut bytes) },
            TermyFfiStatus::Ok
        );
        assert!(bytes.ptr.is_null());

        unsafe {
            (*terminal)
                .terminal
                .hydrate_output(b"\x1b[?1000h\x1b[?1006h");
        }
        assert_eq!(
            unsafe { termy_terminal_encode_mouse(terminal, &input, &mut bytes) },
            TermyFfiStatus::Ok
        );
        let encoded = unsafe { slice::from_raw_parts(bytes.ptr, bytes.len) };
        assert_eq!(encoded, b"\x1b[<0;5;3M");

        assert_eq!(unsafe { termy_buffer_free(bytes) }, TermyFfiStatus::Ok);
        assert_eq!(unsafe { termy_terminal_free(terminal) }, TermyFfiStatus::Ok);
    }
}
