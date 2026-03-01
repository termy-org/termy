use anyhow::{Context, Result, anyhow};
use std::process::Command;

use super::command::tmux_command_line;
use super::snapshot::{parse_session_summaries, session_snapshot_format};
use super::types::{TmuxSessionSummary, TmuxSocketTarget};
#[cfg(test)]
use super::command::quote_tmux_arg;

pub(crate) fn append_socket_args(command: &mut Command, socket_target: &TmuxSocketTarget) {
    if let Some(socket_name) = socket_target.socket_name() {
        command.arg("-L").arg(socket_name);
    }
}

pub(crate) fn run_tmux_command_with_socket(
    binary: &str,
    socket_target: &TmuxSocketTarget,
    args: &[&str],
) -> Result<std::process::Output> {
    let mut command = Command::new(binary);
    append_socket_args(&mut command, socket_target);
    let output = command.args(args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        if stderr.is_empty() {
            return Err(anyhow!(
                "tmux command exited with status {}",
                output
                    .status
                    .code()
                    .map_or_else(|| "signal".to_string(), |code| code.to_string())
            ));
        }
        return Err(anyhow!("{stderr}"));
    }

    Ok(output)
}

pub(crate) fn verify_tmux_version(binary: &str, minimum_major: u8, minimum_minor: u8) -> Result<()> {
    let output = Command::new(binary)
        .arg("-V")
        .output()
        .with_context(|| format!("failed to execute '{}' -V", binary))?;
    if !output.status.success() {
        return Err(anyhow!("'{} -V' failed", binary));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("unable to parse tmux version output: '{}'", stdout.trim()))?;

    let (major, minor) = parse_version_prefix(version)
        .ok_or_else(|| anyhow!("unsupported tmux version format: '{}'", version))?;
    if (major, minor) < (minimum_major, minimum_minor) {
        return Err(anyhow!(
            "tmux {}.{}+ required, found {}",
            minimum_major,
            minimum_minor,
            version
        ));
    }

    Ok(())
}

pub(crate) fn list_sessions(
    binary: &str,
    socket_target: TmuxSocketTarget,
) -> Result<Vec<TmuxSessionSummary>> {
    let format = session_snapshot_format();
    let output = run_tmux_command_with_socket(
        binary,
        &socket_target,
        &["list-sessions", "-F", format],
    )
    .with_context(|| {
        format!(
            "tmux session listing failed: {}",
            tmux_command_line(&["list-sessions", "-F", format])
        )
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_session_summaries(stdout.as_ref())
}

pub(crate) fn rename_session(
    binary: &str,
    socket_target: TmuxSocketTarget,
    current_session_name: &str,
    next_session_name: &str,
) -> Result<()> {
    let current_session_name = current_session_name.trim();
    if current_session_name.is_empty() {
        return Err(anyhow!("tmux current session name cannot be empty"));
    }

    let next_session_name = next_session_name.trim();
    if next_session_name.is_empty() {
        return Err(anyhow!("tmux new session name cannot be empty"));
    }

    run_tmux_command_with_socket(
        binary,
        &socket_target,
        &["rename-session", "-t", current_session_name, next_session_name],
    )
    .with_context(|| {
        format!(
            "tmux session rename failed: {}",
            tmux_command_line(&["rename-session", "-t", current_session_name, next_session_name])
        )
    })?;
    Ok(())
}

pub(crate) fn kill_session(binary: &str, socket_target: TmuxSocketTarget, session_name: &str) -> Result<()> {
    let session_name = session_name.trim();
    if session_name.is_empty() {
        return Err(anyhow!("tmux session name cannot be empty"));
    }

    run_tmux_command_with_socket(binary, &socket_target, &["kill-session", "-t", session_name])
        .with_context(|| {
            format!(
                "tmux session kill failed: {}",
                tmux_command_line(&["kill-session", "-t", session_name])
            )
        })?;
    Ok(())
}

pub(crate) fn parse_version_prefix(version: &str) -> Option<(u8, u8)> {
    let mut digits = String::new();
    for ch in version.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            digits.push(ch);
        } else {
            break;
        }
    }

    let mut parts = digits.split('.');
    let major = parts.next()?.parse::<u8>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u8>().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_session_rejects_empty_session_names() {
        let empty_current = rename_session(
            "tmux",
            TmuxSocketTarget::Default,
            " ",
            "next",
        )
        .expect_err("expected empty current session name failure");
        assert!(
            empty_current
                .to_string()
                .contains("tmux current session name cannot be empty")
        );

        let empty_next = rename_session(
            "tmux",
            TmuxSocketTarget::Default,
            "current",
            " ",
        )
        .expect_err("expected empty next session name failure");
        assert!(
            empty_next
                .to_string()
                .contains("tmux new session name cannot be empty")
        );
    }

    #[test]
    fn kill_session_rejects_empty_session_name() {
        let error = kill_session("tmux", TmuxSocketTarget::Default, " ")
            .expect_err("expected empty session name failure");
        assert!(error.to_string().contains("tmux session name cannot be empty"));
    }

    #[test]
    fn tmux_version_parser_accepts_patch_suffixes() {
        assert_eq!(parse_version_prefix("3.6a"), Some((3, 6)));
        assert_eq!(parse_version_prefix("3.3"), Some((3, 3)));
        assert_eq!(parse_version_prefix("2"), Some((2, 0)));
    }

    #[test]
    fn quote_tmux_arg_preserves_safe_values_without_quotes() {
        assert_eq!(quote_tmux_arg("-L"), "-L");
        assert_eq!(quote_tmux_arg("name_with-safe.chars"), "name_with-safe.chars");
    }

    #[test]
    fn append_socket_args_omits_default_socket() {
        let mut command = Command::new("tmux");
        append_socket_args(&mut command, &TmuxSocketTarget::Default);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.is_empty());
    }

    #[test]
    fn append_socket_args_adds_named_and_dedicated_sockets() {
        let mut dedicated = Command::new("tmux");
        append_socket_args(&mut dedicated, &TmuxSocketTarget::DedicatedTermy);
        let dedicated_args = dedicated
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(dedicated_args, vec!["-L", "termy"]);

        let mut named = Command::new("tmux");
        append_socket_args(&mut named, &TmuxSocketTarget::Named("work".to_string()));
        let named_args = named
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(named_args, vec!["-L", "work"]);
    }
}
