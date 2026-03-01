mod error;
mod io;
mod mutate;

use std::time::Duration;
use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

pub use error::ConfigIoError;
pub use io::{ensure_config_file, open_config_file, subscribe_config_changes};
pub use mutate::{
    import_colors_from_json, remove_root_setting, set_color_setting, set_keybind_lines,
    set_root_setting,
    set_theme_in_config,
};
pub use termy_config_core::{
    AppConfig, ConfigDiagnostic, ConfigDiagnosticKind, CursorStyle, CustomColors,
    PaneFocusEffect, SHELL_DECIDE_THEME_ID, TabCloseVisibility, TabTitleConfig, TabTitleSource,
    TabWidthMode, TerminalScrollbarStyle, TerminalScrollbarVisibility, WorkingDirFallback,
};

pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: AppConfig,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub fingerprint: u64,
}

pub struct RuntimeConfigLoad {
    pub config: AppConfig,
    pub path: Option<PathBuf>,
    pub fingerprint: Option<u64>,
    pub loaded_from_disk: bool,
}

pub(crate) const DEFAULT_CONFIG: &str = termy_config_core::DEFAULT_CONFIG_TEMPLATE;

fn load_from_path(path: PathBuf) -> Result<LoadedConfig, ConfigIoError> {
    let contents = fs::read_to_string(&path).map_err(|source| ConfigIoError::ReadConfig {
        path: path.clone(),
        source,
    })?;
    let fingerprint = config_fingerprint_from_bytes(contents.as_bytes());
    let report = AppConfig::from_contents_with_report(&contents);

    Ok(LoadedConfig {
        path,
        config: report.config,
        diagnostics: report.diagnostics,
        fingerprint,
    })
}

pub fn log_parse_diagnostics(diagnostics: &[ConfigDiagnostic]) {
    for diagnostic in diagnostics {
        log::warn!(
            "Config diagnostic line {} [{:?}]: {}",
            diagnostic.line_number,
            diagnostic.kind,
            diagnostic.message
        );
    }
}

pub fn summarize_parse_diagnostics(diagnostics: &[ConfigDiagnostic]) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }

    let first = &diagnostics[0];
    Some(format!(
        "Config has {} warning(s). First: line {} [{}] {}",
        diagnostics.len(),
        first.line_number,
        diagnostic_kind_label(first.kind),
        first.message
    ))
}

pub fn show_parse_diagnostics_toast(diagnostics: &[ConfigDiagnostic]) {
    let Some(summary) = summarize_parse_diagnostics(diagnostics) else {
        return;
    };

    termy_toast::enqueue_toast(
        termy_toast::ToastKind::Warning,
        summary,
        Some(Duration::from_secs(10)),
    );
}

// This uses Rust's default SipHash-based hasher and is only for in-process
// change detection. Do not persist or compare this fingerprint across
// processes/toolchain versions; use a stable hash algorithm for that.
pub fn config_fingerprint(path: &Path) -> Option<u64> {
    let contents = fs::read(path).ok()?;
    Some(config_fingerprint_from_bytes(&contents))
}

fn config_fingerprint_from_bytes(contents: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    hasher.finish()
}

pub fn report_config_error_once(
    previous_error: &mut Option<String>,
    error_context: &'static str,
    error: &ConfigIoError,
) {
    let error_message = error.to_string();
    if previous_error.as_deref() == Some(error_message.as_str()) {
        return;
    }

    log::error!("{}: {}", error_context, error_message);
    termy_toast::error(error_message.clone());
    *previous_error = Some(error_message);
}

pub fn load_runtime_config(
    previous_error: &mut Option<String>,
    error_context: &'static str,
) -> RuntimeConfigLoad {
    let path = match ensure_config_file() {
        Ok(path) => path,
        Err(error) => {
            report_config_error_once(previous_error, error_context, &error);
            return RuntimeConfigLoad {
                config: AppConfig::default(),
                path: None,
                fingerprint: None,
                loaded_from_disk: false,
            };
        }
    };
    match load_from_path(path.clone()) {
        Ok(loaded) => {
            log_parse_diagnostics(&loaded.diagnostics);
            show_parse_diagnostics_toast(&loaded.diagnostics);
            *previous_error = None;
            RuntimeConfigLoad {
                config: loaded.config,
                path: Some(loaded.path),
                fingerprint: Some(loaded.fingerprint),
                loaded_from_disk: true,
            }
        }
        Err(error) => {
            report_config_error_once(previous_error, error_context, &error);
            RuntimeConfigLoad {
                config: AppConfig::default(),
                path: Some(path),
                fingerprint: None,
                loaded_from_disk: false,
            }
        }
    }
}

fn diagnostic_kind_label(kind: ConfigDiagnosticKind) -> &'static str {
    match kind {
        ConfigDiagnosticKind::UnknownSection => "unknown-section",
        ConfigDiagnosticKind::UnknownRootKey => "unknown-root-key",
        ConfigDiagnosticKind::UnknownColorKey => "unknown-color-key",
        ConfigDiagnosticKind::InvalidSyntax => "invalid-syntax",
        ConfigDiagnosticKind::InvalidValue => "invalid-value",
        ConfigDiagnosticKind::DuplicateRootKey => "duplicate-root-key",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses_without_diagnostics() {
        let report = AppConfig::from_contents_with_report(DEFAULT_CONFIG);
        assert!(
            report.diagnostics.is_empty(),
            "expected no diagnostics but got: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn default_config_template_matches_default_struct() {
        let parsed = AppConfig::from_contents(DEFAULT_CONFIG);
        assert_eq!(parsed, AppConfig::default());
    }

    #[test]
    fn default_config_root_keys_are_known() {
        let mut in_section = false;

        for line in DEFAULT_CONFIG.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_section = true;
                continue;
            }
            if in_section {
                continue;
            }

            let (key, _) = trimmed
                .split_once('=')
                .expect("active config line must contain '='");
            let key = key.trim();
            assert!(
                termy_config_core::VALID_ROOT_KEYS
                    .iter()
                    .any(|valid| valid.eq_ignore_ascii_case(key)),
                "unknown root key in DEFAULT_CONFIG: {}",
                key
            );
        }
    }

    #[test]
    fn default_config_sections_are_known() {
        for line in DEFAULT_CONFIG.lines() {
            let trimmed = line.trim();
            if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
                continue;
            }
            let section_name = trimmed[1..trimmed.len() - 1].trim();
            assert!(
                termy_config_core::VALID_SECTIONS
                    .iter()
                    .any(|section| section.eq_ignore_ascii_case(section_name)),
                "unknown section in DEFAULT_CONFIG: {}",
                section_name
            );
        }
    }
}
