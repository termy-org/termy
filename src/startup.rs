/// Startup failures that should block app launch with actionable recovery guidance.
pub(crate) enum StartupBlocker {
    TmuxPreflight(String),
    TmuxClientLaunch(String),
    TmuxInitialSnapshot(String),
}

impl StartupBlocker {
    pub(crate) fn message(&self) -> String {
        let (reason, error) = match self {
            Self::TmuxPreflight(error) => ("tmux preflight failed", error.as_str()),
            Self::TmuxClientLaunch(error) => ("failed to start tmux control runtime", error.as_str()),
            Self::TmuxInitialSnapshot(error) => ("failed to fetch initial tmux snapshot", error.as_str()),
        };

        format!(
            "Termy cannot continue because {reason}.\n\nError:\n{error}\n\nRecovery:\n- Open your config and set tmux_enabled = false.\n- If tmux integration is desired, set tmux_binary to tmux 3.3 or newer.\n- Save the config and restart Termy."
        )
    }

    pub(crate) fn present_and_exit(self) -> ! {
        let message = self.message();
        termy_native_sdk::show_alert("Termy startup blocked", &message);
        if termy_native_sdk::confirm("Open config?", "Open config file now?") {
            if let Err(error) = crate::app_actions::open_config_file() {
                termy_native_sdk::show_alert("Failed to open config", &error);
            }
        }
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
