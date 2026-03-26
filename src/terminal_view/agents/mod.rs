use super::*;
use alacritty_terminal::grid::Dimensions;
use gpui::{ObjectFit, StatefulInteractiveElement, StyledImage, img, prelude::FluentBuilder};
use libsqlite3_sys as sqlite3;
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod git_panel;
mod interactions;
mod render;
mod status;
#[cfg(test)]
mod tests;
mod types;
mod workspace;
mod workspace_db;

pub(in super::super) use self::types::{
    AgentGitPanelInputMode, AgentGitPanelState, AgentProject, AgentThread,
};
use self::{types::*, workspace_db::*};

const AGENT_WORKSPACE_DB_FILE: &str = "agents.sqlite3";
const LEGACY_AGENT_WORKSPACE_STATE_FILE: &str = "agents.json";
const AGENT_WORKSPACE_SCHEMA_VERSION: u64 = 1;
const LEGACY_AGENT_WORKSPACE_STATE_VERSION: u64 = 1;
const AGENT_WORKSPACE_STATE_ROW_KEY: &str = "state";
const AGENT_SIDEBAR_MIN_WIDTH: f32 = 180.0;
const AGENT_SIDEBAR_MAX_WIDTH: f32 = 500.0;
const AGENT_SIDEBAR_HEADER_HEIGHT: f32 = 30.0;
const AGENT_SIDEBAR_SEARCH_HEIGHT: f32 = 28.0;
const AGENT_SIDEBAR_PROJECT_ROW_HEIGHT: f32 = 24.0;
pub(super) const AGENT_GIT_PANEL_DEFAULT_WIDTH: f32 = 320.0;
const AGENT_GIT_PANEL_MIN_WIDTH: f32 = 220.0;
const AGENT_GIT_PANEL_MAX_WIDTH: f32 = 600.0;
const AGENT_STATUS_VISIBLE_LINE_COUNT: i32 = 6;
static NEXT_AGENT_ENTITY_ID: AtomicU64 = AtomicU64::new(1);

fn next_agent_entity_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let counter = NEXT_AGENT_ENTITY_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{millis}-{counter}")
}

fn now_unix_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

pub(super) fn clamp_agent_sidebar_width(width: f32) -> f32 {
    width.clamp(AGENT_SIDEBAR_MIN_WIDTH, AGENT_SIDEBAR_MAX_WIDTH)
}

pub(super) fn clamp_agent_git_panel_width(width: f32) -> f32 {
    width.clamp(AGENT_GIT_PANEL_MIN_WIDTH, AGENT_GIT_PANEL_MAX_WIDTH)
}

impl TerminalView {
    pub(in super::super) fn agent_sidebar_width(&self) -> f32 {
        if self.should_render_agent_sidebar() {
            self.agent_sidebar_width
        } else {
            0.0
        }
    }

    pub(in super::super) fn terminal_left_sidebar_width(&self) -> f32 {
        self.tab_strip_sidebar_width() + self.agent_sidebar_width()
    }

    pub(in super::super) fn terminal_right_panel_width(&self) -> f32 {
        if self.agent_git_panel.open {
            self.agent_git_panel_width
        } else {
            0.0
        }
    }

    pub(super) fn should_render_agent_sidebar(&self) -> bool {
        self.agent_sidebar_enabled && self.agent_sidebar_open
    }
}
