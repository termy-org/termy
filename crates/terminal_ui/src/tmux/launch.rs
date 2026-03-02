use anyhow::{Context, Result, anyhow};
#[cfg(unix)]
use std::fs::File;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{
    os::fd::{FromRawFd, IntoRawFd},
    process::Stdio,
};

use super::session::append_socket_args;
use super::types::{TmuxLaunchTarget, TmuxRuntimeConfig, TmuxShutdownMode, TmuxSocketTarget};

pub(crate) const PERSISTENT_SESSION_NAME: &str = "termy";

const MANAGED_SESSION_WINDOW_OPTION_BASE_OVERRIDES: [(&str, &str); 2] =
    [("pane-border-status", "off"), ("pane-border-format", "")];
const MANAGED_SESSION_WINDOW_OPTION_ACTIVE_BORDER_OFF_OVERRIDES: [(&str, &str); 3] = [
    ("pane-border-indicators", "off"),
    ("pane-border-style", "fg=default,bg=default"),
    ("pane-active-border-style", "fg=default,bg=default"),
];

#[derive(Debug, Clone)]
pub(crate) struct SessionLaunchPlan {
    pub(crate) session_name: String,
    pub(crate) socket_target: TmuxSocketTarget,
    pub(crate) attach_existing: bool,
    pub(crate) shutdown_mode_on_drop: TmuxShutdownMode,
}

pub(crate) fn launch_plan(config: &TmuxRuntimeConfig) -> SessionLaunchPlan {
    match &config.launch {
        TmuxLaunchTarget::Managed { persistence } => {
            if *persistence {
                SessionLaunchPlan {
                    session_name: PERSISTENT_SESSION_NAME.to_string(),
                    socket_target: TmuxSocketTarget::DedicatedTermy,
                    attach_existing: true,
                    shutdown_mode_on_drop: TmuxShutdownMode::DetachOnly,
                }
            } else {
                SessionLaunchPlan {
                    session_name: managed_session_name(),
                    socket_target: TmuxSocketTarget::DedicatedTermy,
                    attach_existing: false,
                    shutdown_mode_on_drop: TmuxShutdownMode::DetachAndTeardownSession,
                }
            }
        }
        TmuxLaunchTarget::Session { name, socket } => SessionLaunchPlan {
            session_name: name.trim().to_string(),
            socket_target: socket.clone(),
            attach_existing: true,
            shutdown_mode_on_drop: TmuxShutdownMode::DetachOnly,
        },
    }
}

#[cfg(unix)]
pub(crate) fn spawn_tmux_control_mode(
    config: &TmuxRuntimeConfig,
    socket_target: &TmuxSocketTarget,
    session_name: &str,
    attach_existing: bool,
) -> Result<(std::process::Child, File, File)> {
    let pty = rustix_openpty::openpty(None, None)
        .map_err(|error| anyhow!("failed to allocate tmux control pty: {error}"))?;

    let controller = unsafe { File::from_raw_fd(pty.controller.into_raw_fd()) };
    let user = unsafe { File::from_raw_fd(pty.user.into_raw_fd()) };

    let child_stdin = user
        .try_clone()
        .context("failed to clone tmux pty slave for stdin")?;
    let child_stdout = user
        .try_clone()
        .context("failed to clone tmux pty slave for stdout")?;
    let child_stderr = user;

    let mut command = Command::new(config.binary.as_str());
    append_socket_args(&mut command, socket_target);
    command.arg("-CC").arg("new-session");
    if attach_existing {
        command.arg("-A");
    }
    let child = command
        .arg("-s")
        .arg(session_name)
        // tmux windows/panes are authoritative in tmux runtime mode; disable
        // direct shell OSC integration hooks to avoid prompt-width drift artifacts.
        .env("TERMY_SHELL_INTEGRATION", "0")
        .env_remove("TERMY_TAB_TITLE_PREFIX")
        // Avoid inheriting an outer tmux client context; nested `TMUX` can
        // redirect control-mode startup away from the requested session/socket.
        .env_remove("TMUX")
        // zsh can emit inverse PROMPT_EOL_MARK (%) when line-state and repaint diverge.
        // Disable it for tmux-managed shells to avoid persistent visual artifacts.
        .env("PROMPT_EOL_MARK", "")
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn tmux control mode using '{}'",
                config.binary
            )
        })?;

    let writer = controller
        .try_clone()
        .context("failed to clone tmux pty controller for writer")?;

    Ok((child, writer, controller))
}

pub(crate) fn managed_session_window_option_overrides(
    show_active_pane_border: bool,
) -> Vec<(&'static str, &'static str)> {
    let mut overrides = MANAGED_SESSION_WINDOW_OPTION_BASE_OVERRIDES.to_vec();
    if !show_active_pane_border {
        overrides.extend_from_slice(&MANAGED_SESSION_WINDOW_OPTION_ACTIVE_BORDER_OFF_OVERRIDES);
    }
    overrides
}

pub(crate) fn managed_session_window_option_override_commands<'a>(
    all_windows_target: &'a str,
    show_active_pane_border: bool,
) -> impl Iterator<Item = [&'a str; 6]> + 'a {
    managed_session_window_option_overrides(show_active_pane_border)
        .into_iter()
        .map(move |(option, value)| {
            [
                "set-window-option",
                "-q",
                "-t",
                all_windows_target,
                option,
                value,
            ]
        })
}

pub(crate) fn managed_session_name() -> String {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("termy-{}-{}", std::process::id(), now_ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_launch_plan_reuses_fixed_session_without_teardown() {
        let plan = launch_plan(&TmuxRuntimeConfig {
            binary: "tmux".to_string(),
            launch: TmuxLaunchTarget::Managed { persistence: true },
            show_active_pane_border: false,
        });
        assert_eq!(plan.session_name, PERSISTENT_SESSION_NAME);
        assert_eq!(plan.socket_target, TmuxSocketTarget::DedicatedTermy);
        assert!(plan.attach_existing);
        assert_eq!(plan.shutdown_mode_on_drop, TmuxShutdownMode::DetachOnly);
    }

    #[test]
    fn isolated_launch_plan_uses_fresh_session_and_teardown() {
        let plan = launch_plan(&TmuxRuntimeConfig {
            binary: "tmux".to_string(),
            launch: TmuxLaunchTarget::Managed { persistence: false },
            show_active_pane_border: false,
        });
        assert!(plan.session_name.starts_with("termy-"));
        assert_eq!(plan.socket_target, TmuxSocketTarget::DedicatedTermy);
        assert!(!plan.attach_existing);
        assert_eq!(
            plan.shutdown_mode_on_drop,
            TmuxShutdownMode::DetachAndTeardownSession
        );
    }

    #[test]
    fn explicit_session_launch_plan_uses_requested_target_without_teardown() {
        let plan = launch_plan(&TmuxRuntimeConfig {
            binary: "tmux".to_string(),
            launch: TmuxLaunchTarget::Session {
                name: "work".to_string(),
                socket: TmuxSocketTarget::Named("work".to_string()),
            },
            show_active_pane_border: false,
        });
        assert_eq!(plan.session_name, "work");
        assert_eq!(
            plan.socket_target,
            TmuxSocketTarget::Named("work".to_string())
        );
        assert!(plan.attach_existing);
        assert_eq!(plan.shutdown_mode_on_drop, TmuxShutdownMode::DetachOnly);
    }

    #[test]
    fn managed_session_name_prefix_is_stable() {
        let name = managed_session_name();
        assert!(name.starts_with("termy-"));
    }

    #[test]
    fn managed_session_window_option_overrides_include_active_border_neutralization() {
        let overrides = managed_session_window_option_overrides(false);
        assert!(overrides.contains(&("pane-border-status", "off")));
        assert!(overrides.contains(&("pane-border-format", "")));
        assert!(overrides.contains(&("pane-border-indicators", "off")));
        assert!(overrides.contains(&("pane-border-style", "fg=default,bg=default")));
        assert!(overrides.contains(&("pane-active-border-style", "fg=default,bg=default")));
    }

    #[test]
    fn managed_session_window_option_override_commands_include_expected_target_and_flags() {
        let target = "termy:*";
        let commands = managed_session_window_option_override_commands(target, false)
            .collect::<Vec<[&str; 6]>>();
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-status",
            "off",
        ]));
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-format",
            "",
        ]));
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-indicators",
            "off",
        ]));
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-style",
            "fg=default,bg=default",
        ]));
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-active-border-style",
            "fg=default,bg=default",
        ]));
    }

    #[test]
    fn managed_session_window_option_overrides_skip_active_border_neutralization_when_enabled() {
        let overrides = managed_session_window_option_overrides(true);
        assert!(overrides.contains(&("pane-border-status", "off")));
        assert!(overrides.contains(&("pane-border-format", "")));
        assert!(!overrides.contains(&("pane-border-indicators", "off")));
        assert!(!overrides.contains(&("pane-border-style", "fg=default,bg=default")));
        assert!(!overrides.contains(&("pane-active-border-style", "fg=default,bg=default")));
    }

    #[test]
    fn managed_session_window_option_override_commands_skip_active_border_neutralization_when_enabled()
     {
        let target = "termy:*";
        let commands = managed_session_window_option_override_commands(target, true)
            .collect::<Vec<[&str; 6]>>();
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-status",
            "off",
        ]));
        assert!(commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-format",
            "",
        ]));
        assert!(!commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-indicators",
            "off",
        ]));
        assert!(!commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-border-style",
            "fg=default,bg=default",
        ]));
        assert!(!commands.contains(&[
            "set-window-option",
            "-q",
            "-t",
            target,
            "pane-active-border-style",
            "fg=default,bg=default",
        ]));
    }
}
