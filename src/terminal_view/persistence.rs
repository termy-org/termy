use super::*;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

const NATIVE_WORKSPACE_STATE_VERSION: u64 = 2;
const NATIVE_WORKSPACE_STATE_FILE: &str = "native-tabs.json";

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedNativePane {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    buffer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedNativeTab {
    panes: Vec<PersistedNativePane>,
    active_pane: usize,
    manual_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedNativeWorkspace {
    tabs: Vec<PersistedNativeTab>,
    active_tab: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PersistedNamedLayout {
    name: String,
    workspace: PersistedNativeWorkspace,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct PersistedNativeWorkspaceState {
    last_session: Option<PersistedNativeWorkspace>,
    layouts: Vec<PersistedNamedLayout>,
}

#[derive(Clone, Debug)]
struct PersistedNativeWorkspaceWriteRequest {
    path: PathBuf,
    workspace: PersistedNativeWorkspace,
    current_named_layout: Option<String>,
    persist_last_session: bool,
    autosave_named_layout: bool,
}

impl TerminalView {
    fn extract_persisted_buffer_line(
        grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
        line_idx: i32,
    ) -> Option<String> {
        let line = Line(line_idx);
        let cols = grid.columns();
        let total_lines = grid.total_lines();
        if line_idx < -(total_lines as i32 - grid.screen_lines() as i32)
            || line_idx >= grid.screen_lines() as i32
        {
            return None;
        }

        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &grid[line][Column(col)];
            let c = cell.c;
            if c == '\0' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) || c.is_control() {
                text.push(' ');
            } else {
                text.push(c);
            }
        }

        Some(text.trim_end().to_string())
    }

    fn extract_persisted_buffer_text(&self, terminal: &Terminal) -> Option<String> {
        if !self.native_buffer_persistence {
            return None;
        }

        let (_, history_size) = terminal.scroll_state();
        let rows = i32::from(terminal.size().rows);
        terminal.with_grid(|grid| {
            let mut lines = Vec::with_capacity(history_size.saturating_add(rows as usize));
            for line_idx in -(history_size as i32)..rows {
                if let Some(text) = Self::extract_persisted_buffer_line(grid, line_idx) {
                    lines.push(text);
                }
            }
            let joined = lines.join("\r\n");
            (!joined.trim().is_empty()).then_some(joined)
        })?
    }

    fn should_sync_persisted_native_workspace(&self) -> bool {
        self.runtime_kind() == RuntimeKind::Native
            && (self.native_tab_persistence
                || (self.native_layout_autosave && self.current_named_layout.is_some()))
    }

    fn persisted_native_workspace_path() -> Result<PathBuf, String> {
        let config_path = crate::config::ensure_config_file().map_err(|error| error.to_string())?;
        let parent = config_path
            .parent()
            .ok_or_else(|| format!("Invalid config path '{}'", config_path.display()))?;
        Ok(parent.join(NATIVE_WORKSPACE_STATE_FILE))
    }

    fn load_persisted_native_workspace_state_from_path(
        path: &std::path::Path,
    ) -> Result<PersistedNativeWorkspaceState, String> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PersistedNativeWorkspaceState::default());
            }
            Err(error) => {
                return Err(format!(
                    "Failed to read workspace state '{}': {}",
                    path.display(),
                    error
                ));
            }
        };

        Self::parse_persisted_native_workspace_state(&contents)
    }

    fn store_persisted_native_workspace_state_to_path(
        path: &std::path::Path,
        state: PersistedNativeWorkspaceState,
    ) -> Result<(), String> {
        if state.last_session.is_none() && state.layouts.is_empty() {
            match fs::remove_file(path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => {
                    return Err(format!(
                        "Failed to remove workspace state '{}': {}",
                        path.display(),
                        error
                    ));
                }
            }
        }

        let contents = serde_json::to_string_pretty(&json!({
            "version": NATIVE_WORKSPACE_STATE_VERSION,
            "last_session": state.last_session.map(Self::persisted_workspace_to_value),
            "layouts": state.layouts.into_iter().map(|layout| {
                json!({
                    "name": layout.name,
                    "workspace": Self::persisted_workspace_to_value(layout.workspace),
                })
            }).collect::<Vec<_>>(),
        }))
        .map_err(|error| format!("Failed to encode native tab workspace: {}", error))?;
        Self::write_persisted_native_workspace_atomically(path, &contents)
    }

    fn write_persisted_native_workspace_atomically(
        path: &std::path::Path,
        contents: &str,
    ) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| format!("Invalid workspace state path '{}'", path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {}", parent.display(), error))?;
        let mut temp = NamedTempFile::new_in(parent).map_err(|error| {
            format!(
                "Failed to create temp file in '{}': {}",
                parent.display(),
                error
            )
        })?;
        temp.write_all(contents.as_bytes())
            .map_err(|error| format!("Failed to write workspace state: {}", error))?;
        temp.flush()
            .map_err(|error| format!("Failed to flush workspace state: {}", error))?;
        temp.as_file()
            .sync_all()
            .map_err(|error| format!("Failed to sync workspace state: {}", error))?;
        temp.persist(path).map_err(|error| {
            format!(
                "Failed to persist workspace state '{}': {}",
                path.display(),
                error.error
            )
        })?;
        Ok(())
    }

    fn persisted_native_workspace_working_dir(&self) -> Option<String> {
        Self::resolve_configured_working_directory(self.configured_working_dir.as_deref())
            .or_else(|| {
                Self::default_working_directory_with_fallback(
                    self.terminal_runtime.working_dir_fallback,
                )
            })
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn collect_persisted_native_workspace(&self) -> Option<PersistedNativeWorkspace> {
        if self.runtime_kind() != RuntimeKind::Native || self.tabs.is_empty() {
            return None;
        }

        let tabs = self
            .tabs
            .iter()
            .map(|tab| PersistedNativeTab {
                panes: tab
                    .panes
                    .iter()
                    .map(|pane| PersistedNativePane {
                        left: pane.left,
                        top: pane.top,
                        width: pane.width.max(1),
                        height: pane.height.max(1),
                        buffer: self.extract_persisted_buffer_text(&pane.terminal),
                    })
                    .collect(),
                active_pane: tab.active_pane_index().unwrap_or(0),
                manual_title: tab.manual_title.clone(),
            })
            .collect::<Vec<_>>();

        Some(PersistedNativeWorkspace {
            tabs,
            active_tab: self.active_tab.min(self.tabs.len().saturating_sub(1)),
        })
    }

    fn persisted_workspace_to_value(workspace: PersistedNativeWorkspace) -> Value {
        json!({
            "active_tab": workspace.active_tab,
            "tabs": workspace.tabs.into_iter().map(|tab| {
                json!({
                    "active_pane": tab.active_pane,
                    "manual_title": tab.manual_title,
                    "panes": tab.panes.into_iter().map(|pane| {
                        json!({
                            "left": pane.left,
                            "top": pane.top,
                            "width": pane.width,
                            "height": pane.height,
                            "buffer": pane.buffer,
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        })
    }

    fn parse_persisted_native_workspace_value(
        root: &Value,
    ) -> Result<PersistedNativeWorkspace, String> {
        fn value_u16(value: &Value, field: &str) -> Result<u16, String> {
            let raw = value.as_u64().ok_or_else(|| {
                format!("workspace field '{}' must be an unsigned integer", field)
            })?;
            u16::try_from(raw).map_err(|_| format!("workspace field '{}' exceeds u16 range", field))
        }

        let tabs_value = root
            .get("tabs")
            .and_then(Value::as_array)
            .ok_or_else(|| "workspace state is missing 'tabs'".to_string())?;
        let mut tabs = Vec::with_capacity(tabs_value.len());
        for (tab_index, tab_value) in tabs_value.iter().enumerate() {
            let panes_value = tab_value
                .get("panes")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("workspace tab {} is missing 'panes'", tab_index))?;
            if panes_value.is_empty() {
                continue;
            }

            let mut panes = Vec::with_capacity(panes_value.len());
            for (pane_index, pane_value) in panes_value.iter().enumerate() {
                panes.push(PersistedNativePane {
                    left: value_u16(
                        pane_value.get("left").ok_or_else(|| {
                            format!(
                                "workspace tab {} pane {} is missing 'left'",
                                tab_index, pane_index
                            )
                        })?,
                        "left",
                    )?,
                    top: value_u16(
                        pane_value.get("top").ok_or_else(|| {
                            format!(
                                "workspace tab {} pane {} is missing 'top'",
                                tab_index, pane_index
                            )
                        })?,
                        "top",
                    )?,
                    width: value_u16(
                        pane_value.get("width").ok_or_else(|| {
                            format!(
                                "workspace tab {} pane {} is missing 'width'",
                                tab_index, pane_index
                            )
                        })?,
                        "width",
                    )?
                    .max(1),
                    height: value_u16(
                        pane_value.get("height").ok_or_else(|| {
                            format!(
                                "workspace tab {} pane {} is missing 'height'",
                                tab_index, pane_index
                            )
                        })?,
                        "height",
                    )?
                    .max(1),
                    buffer: pane_value
                        .get("buffer")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .filter(|buffer| !buffer.is_empty()),
                });
            }

            let active_pane = tab_value
                .get("active_pane")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0)
                .min(panes.len().saturating_sub(1));
            let manual_title = tab_value
                .get("manual_title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|title| !title.trim().is_empty());
            tabs.push(PersistedNativeTab {
                panes,
                active_pane,
                manual_title,
            });
        }

        if tabs.is_empty() {
            return Err("workspace state does not contain any tabs".to_string());
        }

        let active_tab = root
            .get("active_tab")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0)
            .min(tabs.len().saturating_sub(1));

        Ok(PersistedNativeWorkspace { tabs, active_tab })
    }

    fn parse_persisted_native_workspace_state(
        contents: &str,
    ) -> Result<PersistedNativeWorkspaceState, String> {
        let root: Value = serde_json::from_str(contents)
            .map_err(|error| format!("Invalid native tab workspace JSON: {}", error))?;
        let version = root
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "workspace state is missing 'version'".to_string())?;

        match version {
            1 => {
                let workspace = Self::parse_persisted_native_workspace_value(&root)?;
                Ok(PersistedNativeWorkspaceState {
                    last_session: Some(workspace),
                    layouts: Vec::new(),
                })
            }
            2 => {
                let last_session = root
                    .get("last_session")
                    .filter(|value| !value.is_null())
                    .map(Self::parse_persisted_native_workspace_value)
                    .transpose()?;
                let layouts_value = root
                    .get("layouts")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "workspace state is missing 'layouts'".to_string())?;
                let mut layouts = Vec::with_capacity(layouts_value.len());
                for (layout_index, layout_value) in layouts_value.iter().enumerate() {
                    let name = layout_value
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| format!("saved layout {} is missing 'name'", layout_index))?
                        .to_string();
                    let workspace = Self::parse_persisted_native_workspace_value(
                        layout_value.get("workspace").ok_or_else(|| {
                            format!("saved layout '{}' is missing 'workspace'", name)
                        })?,
                    )?;
                    layouts.push(PersistedNamedLayout { name, workspace });
                }
                layouts.sort_unstable_by_key(|layout| layout.name.to_ascii_lowercase());
                layouts.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
                Ok(PersistedNativeWorkspaceState {
                    last_session,
                    layouts,
                })
            }
            _ => Err(format!("Unsupported workspace state version {}", version)),
        }
    }

    fn load_persisted_native_workspace_state(
        &self,
    ) -> Result<PersistedNativeWorkspaceState, String> {
        let path = Self::persisted_native_workspace_path()?;
        Self::load_persisted_native_workspace_state_from_path(&path)
    }

    fn store_persisted_native_workspace_state(
        state: PersistedNativeWorkspaceState,
    ) -> Result<(), String> {
        let path = Self::persisted_native_workspace_path()?;
        Self::store_persisted_native_workspace_state_to_path(&path, state)
    }

    fn restore_workspace(
        &mut self,
        workspace: PersistedNativeWorkspace,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let working_dir = self.persisted_native_workspace_working_dir();
        let predicted_prompt_cwd = Self::predicted_prompt_cwd(
            working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        );
        let predicted_title =
            Self::predicted_prompt_seed_title(&self.tab_title, predicted_prompt_cwd.as_deref());
        let mut restored_tabs = Vec::with_capacity(workspace.tabs.len());

        for persisted_tab in workspace.tabs {
            let first_pane = persisted_tab
                .panes
                .first()
                .ok_or_else(|| "workspace tab is missing panes".to_string())?;
            let first_terminal = Terminal::new_native(
                TerminalSize {
                    cols: first_pane.width.max(1),
                    rows: first_pane.height.max(1),
                    ..TerminalSize::default()
                },
                working_dir.as_deref(),
                Some(self.event_wakeup_tx.clone()),
                Some(&self.tab_shell_integration),
                Some(&self.terminal_runtime),
            )
            .map_err(|error| format!("Failed to restore saved tab: {}", error))?;
            let tab_id = self.allocate_tab_id();
            let mut tab = Self::create_native_tab(
                tab_id,
                first_terminal,
                first_pane.width,
                first_pane.height,
                predicted_title.clone(),
            );
            if let Some(first) = tab.panes.first_mut() {
                first.left = first_pane.left;
                first.top = first_pane.top;
                first.width = first_pane.width.max(1);
                first.height = first_pane.height.max(1);
                if self.native_buffer_persistence
                    && let Some(buffer) = first_pane.buffer.as_deref()
                {
                    first.terminal.hydrate_output(buffer.as_bytes());
                }
            }

            for (pane_index, pane) in persisted_tab.panes.iter().enumerate().skip(1) {
                let terminal = Terminal::new_native(
                    TerminalSize {
                        cols: pane.width.max(1),
                        rows: pane.height.max(1),
                        ..TerminalSize::default()
                    },
                    working_dir.as_deref(),
                    Some(self.event_wakeup_tx.clone()),
                    Some(&self.tab_shell_integration),
                    Some(&self.terminal_runtime),
                )
                .map_err(|error| format!("Failed to restore saved pane: {}", error))?;
                if self.native_buffer_persistence
                    && let Some(buffer) = pane.buffer.as_deref()
                {
                    terminal.hydrate_output(buffer.as_bytes());
                }
                tab.panes.push(TerminalPane {
                    id: format!("%native-restored-{tab_id}-{}", pane_index + 1),
                    left: pane.left,
                    top: pane.top,
                    width: pane.width.max(1),
                    height: pane.height.max(1),
                    degraded: false,
                    terminal,
                    render_cache: RefCell::new(TerminalPaneRenderCache::default()),
                });
            }

            tab.active_pane_id = tab
                .panes
                .get(persisted_tab.active_pane)
                .or_else(|| tab.panes.first())
                .map(|pane| pane.id.clone())
                .ok_or_else(|| "restored tab has no panes".to_string())?;
            tab.manual_title = persisted_tab.manual_title;
            restored_tabs.push(tab);
        }

        if restored_tabs.is_empty() {
            return Err("workspace state does not contain any restorable tabs".to_string());
        }

        self.tabs = restored_tabs;
        self.active_tab = workspace.active_tab.min(self.tabs.len().saturating_sub(1));
        self.mark_tab_strip_layout_dirty();
        self.sync_tab_strip_for_active_tab();
        for index in 0..self.tabs.len() {
            self.refresh_tab_title(index);
        }
        self.clear_selection();
        self.clear_hovered_link();
        cx.notify();
        Ok(())
    }

    fn apply_persisted_native_workspace_write_request(
        request: PersistedNativeWorkspaceWriteRequest,
    ) -> Result<(), String> {
        let mut state = Self::load_persisted_native_workspace_state_from_path(&request.path)?;
        if request.autosave_named_layout
            && let Some(current_named_layout) = request.current_named_layout.as_deref()
            && let Some(layout) = state
                .layouts
                .iter_mut()
                .find(|layout| layout.name.eq_ignore_ascii_case(current_named_layout))
        {
            layout.workspace = request.workspace.clone();
        }
        if request.persist_last_session {
            state.last_session = Some(request.workspace);
        }
        Self::store_persisted_native_workspace_state_to_path(&request.path, state)
    }

    fn persisted_native_workspace_write_request(
        &self,
    ) -> Option<PersistedNativeWorkspaceWriteRequest> {
        if !self.should_sync_persisted_native_workspace() {
            return None;
        }
        let workspace = self.collect_persisted_native_workspace()?;
        let path = match Self::persisted_native_workspace_path() {
            Ok(path) => path,
            Err(error) => {
                log::error!("Failed to resolve native workspace state path: {}", error);
                return None;
            }
        };
        Some(PersistedNativeWorkspaceWriteRequest {
            path,
            workspace,
            current_named_layout: self.current_named_layout.clone(),
            persist_last_session: self.native_tab_persistence,
            autosave_named_layout: self.native_layout_autosave,
        })
    }

    pub(in super::super) fn sync_persisted_native_workspace(&self) {
        let Some(request) = self.persisted_native_workspace_write_request() else {
            return;
        };
        if let Err(error) = Self::apply_persisted_native_workspace_write_request(request) {
            log::error!("Failed to persist native tab workspace: {}", error);
        }
    }

    pub(in super::super) fn schedule_persist_native_workspace(&self) {
        let Some(request) = self.persisted_native_workspace_write_request() else {
            return;
        };
        let next_revision = self
            .native_persist_revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1);
        let latest_revision = self.native_persist_revision.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            if latest_revision.load(std::sync::atomic::Ordering::Acquire) != next_revision {
                return;
            }
            if let Err(error) =
                TerminalView::apply_persisted_native_workspace_write_request(request)
            {
                log::error!("Failed to persist native tab workspace: {}", error);
            }
        });
    }

    pub(in super::super) fn clear_persisted_native_workspace(&self) -> Result<(), String> {
        let path = Self::persisted_native_workspace_path()?;
        let mut state = Self::load_persisted_native_workspace_state_from_path(&path)
            .unwrap_or_else(|_| PersistedNativeWorkspaceState::default());
        state.last_session = None;
        Self::store_persisted_native_workspace_state_to_path(&path, state)
    }

    pub(in super::super) fn rewrite_persisted_native_workspace_without_buffers(
        &self,
    ) -> Result<(), String> {
        let path = Self::persisted_native_workspace_path()?;
        let mut state = self.load_persisted_native_workspace_state()?;
        let clear_buffers = |workspace: &mut PersistedNativeWorkspace| {
            for tab in &mut workspace.tabs {
                for pane in &mut tab.panes {
                    pane.buffer = None;
                }
            }
        };
        if let Some(last_session) = state.last_session.as_mut() {
            clear_buffers(last_session);
        }
        for layout in &mut state.layouts {
            clear_buffers(&mut layout.workspace);
        }
        Self::store_persisted_native_workspace_state_to_path(&path, state)
    }

    pub(in super::super) fn saved_layout_names(&self) -> Result<Vec<String>, String> {
        let state = self.load_persisted_native_workspace_state()?;
        Ok(state
            .layouts
            .into_iter()
            .map(|layout| layout.name)
            .collect())
    }

    pub(in super::super) fn save_current_workspace_as_named_layout(
        &mut self,
        layout_name: &str,
    ) -> Result<(), String> {
        if self.runtime_kind() != RuntimeKind::Native {
            return Err("Saved layouts are only available in the native runtime".to_string());
        }
        let layout_name = layout_name.trim();
        if layout_name.is_empty() {
            return Err("Layout name is required".to_string());
        }
        let workspace = self
            .collect_persisted_native_workspace()
            .ok_or_else(|| "There is no native layout to save".to_string())?;
        let mut state = self.load_persisted_native_workspace_state()?;
        if let Some(existing) = state
            .layouts
            .iter_mut()
            .find(|layout| layout.name.eq_ignore_ascii_case(layout_name))
        {
            existing.name = layout_name.to_string();
            existing.workspace = workspace;
        } else {
            state.layouts.push(PersistedNamedLayout {
                name: layout_name.to_string(),
                workspace,
            });
            state
                .layouts
                .sort_unstable_by_key(|layout| layout.name.to_ascii_lowercase());
        }
        Self::store_persisted_native_workspace_state(state)?;
        self.current_named_layout = Some(layout_name.to_string());
        Ok(())
    }

    pub(in super::super) fn load_named_layout(
        &mut self,
        layout_name: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.runtime_kind() != RuntimeKind::Native {
            return Err("Saved layouts are only available in the native runtime".to_string());
        }
        let state = self.load_persisted_native_workspace_state()?;
        let layout = state
            .layouts
            .into_iter()
            .find(|layout| layout.name.eq_ignore_ascii_case(layout_name))
            .ok_or_else(|| format!("Saved layout \"{}\" was not found", layout_name))?;
        self.restore_workspace(layout.workspace, cx)?;
        self.current_named_layout = Some(layout.name);
        self.sync_persisted_native_workspace();
        Ok(())
    }

    pub(in super::super) fn rename_named_layout(
        &mut self,
        current_layout_name: &str,
        next_layout_name: &str,
    ) -> Result<(), String> {
        let current_layout_name = current_layout_name.trim();
        let next_layout_name = next_layout_name.trim();
        if current_layout_name.is_empty() || next_layout_name.is_empty() {
            return Err("Layout name is required".to_string());
        }

        let mut state = self.load_persisted_native_workspace_state()?;
        if state.layouts.iter().any(|layout| {
            layout.name.eq_ignore_ascii_case(next_layout_name)
                && !layout.name.eq_ignore_ascii_case(current_layout_name)
        }) {
            return Err(format!(
                "A saved layout named \"{}\" already exists",
                next_layout_name
            ));
        }
        let layout = state
            .layouts
            .iter_mut()
            .find(|layout| layout.name.eq_ignore_ascii_case(current_layout_name))
            .ok_or_else(|| format!("Saved layout \"{}\" was not found", current_layout_name))?;
        layout.name = next_layout_name.to_string();
        let update_current_named_layout = self
            .current_named_layout
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(current_layout_name));
        state
            .layouts
            .sort_unstable_by_key(|candidate| candidate.name.to_ascii_lowercase());
        Self::store_persisted_native_workspace_state(state)?;
        if update_current_named_layout {
            self.current_named_layout = Some(next_layout_name.to_string());
        }
        Ok(())
    }

    pub(in super::super) fn delete_named_layout(
        &mut self,
        layout_name: &str,
    ) -> Result<(), String> {
        let layout_name = layout_name.trim();
        if layout_name.is_empty() {
            return Err("Layout name is required".to_string());
        }
        let mut state = self.load_persisted_native_workspace_state()?;
        let previous_len = state.layouts.len();
        state
            .layouts
            .retain(|layout| !layout.name.eq_ignore_ascii_case(layout_name));
        if state.layouts.len() == previous_len {
            return Err(format!("Saved layout \"{}\" was not found", layout_name));
        }
        let clear_current_named_layout = self
            .current_named_layout
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(layout_name));
        Self::store_persisted_native_workspace_state(state)?;
        if clear_current_named_layout {
            self.current_named_layout = None;
        }
        Ok(())
    }

    pub(in super::super) fn restore_persisted_native_workspace(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.runtime_kind() != RuntimeKind::Native || !self.native_tab_persistence {
            return Ok(false);
        }
        let state = self.load_persisted_native_workspace_state()?;
        let Some(workspace) = state.last_session else {
            return Ok(false);
        };
        self.restore_workspace(workspace, cx)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalView;

    #[test]
    fn persisted_native_workspace_parser_accepts_legacy_v1_shape() {
        let state = TerminalView::parse_persisted_native_workspace_state(
            r#"{
  "version": 1,
  "active_tab": 1,
  "tabs": [
    {
      "active_pane": 0,
      "manual_title": "Work",
      "panes": [
        { "left": 0, "top": 0, "width": 60, "height": 20 }
      ]
    },
    {
      "active_pane": 1,
      "manual_title": null,
      "panes": [
        { "left": 0, "top": 0, "width": 40, "height": 20 },
        { "left": 40, "top": 0, "width": 40, "height": 20 }
      ]
    }
  ]
}"#,
        )
        .expect("workspace should parse");

        let workspace = state
            .last_session
            .expect("legacy state should populate last session");
        assert_eq!(workspace.tabs.len(), 2);
        assert_eq!(workspace.active_tab, 1);
        assert_eq!(workspace.tabs[0].manual_title.as_deref(), Some("Work"));
        assert_eq!(workspace.tabs[1].active_pane, 1);
        assert_eq!(workspace.tabs[1].panes[1].left, 40);
    }

    #[test]
    fn persisted_native_workspace_parser_accepts_named_layouts() {
        let state = TerminalView::parse_persisted_native_workspace_state(
            r#"{
  "version": 2,
  "last_session": null,
  "layouts": [
    {
      "name": "Main",
      "workspace": {
        "active_tab": 0,
        "tabs": [
          {
            "active_pane": 0,
            "manual_title": null,
            "panes": [
              { "left": 0, "top": 0, "width": 80, "height": 24 }
            ]
          }
        ]
      }
    }
  ]
}"#,
        )
        .expect("workspace state should parse");

        assert!(state.last_session.is_none());
        assert_eq!(state.layouts.len(), 1);
        assert_eq!(state.layouts[0].name, "Main");
        assert_eq!(state.layouts[0].workspace.tabs.len(), 1);
    }

    #[test]
    fn persisted_native_workspace_parser_rejects_unknown_version() {
        let error = TerminalView::parse_persisted_native_workspace_state(
            r#"{"version":99,"last_session":null,"layouts":[]}"#,
        )
        .expect_err("unexpected parser success");

        assert!(error.contains("Unsupported workspace state version"));
    }
}
