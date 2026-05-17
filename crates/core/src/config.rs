use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use crate::protocol::TerminalQueryColors;
use crate::runtime::{
    TerminalCursorStyle, TerminalRuntimeConfig, WorkingDirFallback as RuntimeWorkingDirFallback,
};
use termy_config_core::{AppConfig, ConfigDiagnostic, CursorStyle, WorkingDirFallback};

#[derive(Debug, Clone)]
pub struct LoadedTermyConfig {
    pub path: Option<PathBuf>,
    pub app_config: AppConfig,
    pub runtime_config: TerminalRuntimeConfig,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub loaded_from_disk: bool,
}

#[derive(Debug)]
pub enum TermyConfigError {
    Read { path: PathBuf, source: io::Error },
}

impl fmt::Display for TermyConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
        }
    }
}

impl Error for TermyConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
        }
    }
}

pub fn load_config_from_default_path() -> Result<LoadedTermyConfig, TermyConfigError> {
    let Some(path) = termy_config_core::config_path() else {
        return Ok(default_loaded_config(None));
    };

    match load_config_from_path(&path) {
        Ok(config) => Ok(config),
        Err(TermyConfigError::Read { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            Ok(default_loaded_config(Some(path)))
        }
        Err(error) => Err(error),
    }
}

pub fn load_config_from_path(
    path: impl AsRef<Path>,
) -> Result<LoadedTermyConfig, TermyConfigError> {
    let path = path.as_ref().to_path_buf();
    let contents = fs::read_to_string(&path).map_err(|source| TermyConfigError::Read {
        path: path.clone(),
        source,
    })?;
    Ok(load_config_from_contents_with_path(
        &contents,
        Some(path),
        true,
    ))
}

pub fn load_config_from_contents(contents: &str) -> LoadedTermyConfig {
    load_config_from_contents_with_path(contents, None, false)
}

pub fn runtime_config_from_app_config(config: &AppConfig) -> TerminalRuntimeConfig {
    runtime_config_from_app_config_with_query_colors(config, TerminalQueryColors::default())
}

pub fn runtime_config_from_app_config_with_query_colors(
    config: &AppConfig,
    query_colors: TerminalQueryColors,
) -> TerminalRuntimeConfig {
    let working_dir_fallback = match config.working_dir_fallback {
        WorkingDirFallback::Home => RuntimeWorkingDirFallback::Home,
        WorkingDirFallback::Process => RuntimeWorkingDirFallback::Process,
    };

    TerminalRuntimeConfig {
        shell: config.shell.clone(),
        term: config.term.clone(),
        colorterm: config.colorterm.clone(),
        query_colors,
        working_dir_fallback,
        scrollback_history: config.scrollback_history,
        default_cursor_style: match config.cursor_style {
            CursorStyle::Line => TerminalCursorStyle::Line,
            CursorStyle::Block => TerminalCursorStyle::Block,
        },
    }
}

fn load_config_from_contents_with_path(
    contents: &str,
    path: Option<PathBuf>,
    loaded_from_disk: bool,
) -> LoadedTermyConfig {
    let report = AppConfig::from_contents_with_report(contents);
    LoadedTermyConfig {
        path,
        runtime_config: runtime_config_from_app_config(&report.config),
        app_config: report.config,
        diagnostics: report.diagnostics,
        loaded_from_disk,
    }
}

fn default_loaded_config(path: Option<PathBuf>) -> LoadedTermyConfig {
    let app_config = AppConfig::default();
    LoadedTermyConfig {
        path,
        runtime_config: runtime_config_from_app_config(&app_config),
        app_config,
        diagnostics: Vec::new(),
        loaded_from_disk: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_runtime_fields_from_contents() {
        let loaded = load_config_from_contents(
            "shell = /bin/zsh\nterm = screen-256color\ncolorterm = none\nscrollback = 42\ncursor_style = line\n",
        );

        assert_eq!(loaded.runtime_config.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(loaded.runtime_config.term, "screen-256color");
        assert_eq!(loaded.runtime_config.colorterm, None);
        assert_eq!(loaded.runtime_config.scrollback_history, 42);
        assert_eq!(
            loaded.runtime_config.default_cursor_style,
            TerminalCursorStyle::Line
        );
        assert!(!loaded.loaded_from_disk);
    }

    #[test]
    fn loads_explicit_path_with_diagnostics() {
        let tempdir = tempdir().expect("tempdir");
        let path = tempdir.path().join("config.txt");
        fs::write(&path, "unknown_key = true\n").expect("write config");

        let loaded = load_config_from_path(&path).expect("load config");

        assert_eq!(loaded.path.as_deref(), Some(path.as_path()));
        assert!(loaded.loaded_from_disk);
        assert_eq!(loaded.diagnostics.len(), 1);
    }

    #[test]
    fn missing_explicit_path_is_an_error() {
        let tempdir = tempdir().expect("tempdir");
        let path = tempdir.path().join("missing.txt");

        let error = load_config_from_path(&path).expect_err("missing path should fail");

        match error {
            TermyConfigError::Read {
                path: error_path,
                source,
            } => {
                assert_eq!(error_path, path);
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
        }
    }
}
