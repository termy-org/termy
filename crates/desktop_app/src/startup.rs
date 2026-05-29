/// Startup failures that should block app launch with actionable recovery guidance.
#[allow(clippy::enum_variant_names)]
pub(crate) enum StartupBlocker {
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    TmuxPreflight(String),
    TmuxClientLaunch(String),
    TmuxInitialSnapshot(String),
}

impl StartupBlocker {
    pub(crate) fn message(&self) -> String {
        let (reason, error) = match self {
            Self::TmuxPreflight(error) => ("tmux preflight failed", error.as_str()),
            Self::TmuxClientLaunch(error) => {
                ("failed to start tmux control runtime", error.as_str())
            }
            Self::TmuxInitialSnapshot(error) => {
                ("failed to fetch initial tmux snapshot", error.as_str())
            }
        };

        format!(
            "Termy cannot continue because {reason}.\n\nError:\n{error}\n\nRecovery:\n- Open your config and set tmux_enabled = false to start in native mode.\n- Finder/DMG launches use a minimal environment; set tmux_binary to an absolute path (for example /opt/homebrew/bin/tmux) if tmux is not on the default PATH.\n- If tmux integration is desired, ensure tmux 3.3 or newer is installed.\n- Save the config and restart Termy, then use tmux Sessions… when ready."
        )
    }

    pub(crate) fn present_and_exit(self) -> ! {
        let message = self.message();
        // Startup blockers can fire while GPUI holds internal borrows during app/window
        // initialization. Triggering synchronous native modal dialogs in that state can
        // re-enter GPUI and panic; keep this path side-effect free and terminate cleanly.
        eprintln!("Termy startup blocked:\n{message}");
        // Hard cutover: do not continue startup after tmux preflight/startup failures.
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::StartupBlocker;

    #[test]
    fn startup_blocker_message_includes_tmux_guidance() {
        let message = StartupBlocker::TmuxPreflight("tmux 3.3+ required".to_string()).message();
        assert!(message.contains("tmux 3.3+ required"));
        assert!(message.contains("tmux_enabled"));
        assert!(message.contains("tmux Sessions…"));
        assert!(message.contains("Finder/DMG"));
        assert!(message.contains("/opt/homebrew/bin/tmux"));
        assert!(message.contains("restart"));
    }

    #[test]
    fn startup_blocker_message_for_tmux_client_launch_includes_exact_error() {
        let message = StartupBlocker::TmuxClientLaunch("socket unavailable".to_string()).message();
        assert!(message.contains("failed to start tmux control runtime"));
        assert!(message.contains("socket unavailable"));
    }

    #[test]
    fn startup_blocker_message_for_initial_snapshot_includes_exact_error() {
        let message =
            StartupBlocker::TmuxInitialSnapshot("list-windows failed".to_string()).message();
        assert!(message.contains("failed to fetch initial tmux snapshot"));
        assert!(message.contains("list-windows failed"));
    }
}
