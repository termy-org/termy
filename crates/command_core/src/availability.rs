use crate::CommandId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandCapabilities {
    pub tmux_runtime_active: bool,
    pub install_cli_available: bool,
    pub ai_features_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandUnavailableReason {
    RequiresTmuxRuntime,
    InstallCliAlreadyInstalled,
    AiFeaturesDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandAvailability {
    pub enabled: bool,
    pub reason: Option<CommandUnavailableReason>,
}

impl CommandId {
    pub const fn is_ai_only(self) -> bool {
        matches!(self, Self::ToggleAiInput | Self::ToggleChatSidebar)
    }

    pub const fn availability(self, caps: CommandCapabilities) -> CommandAvailability {
        if self.is_tmux_only() && !caps.tmux_runtime_active {
            return CommandAvailability {
                enabled: false,
                reason: Some(CommandUnavailableReason::RequiresTmuxRuntime),
            };
        }

        if matches!(self, Self::InstallCli) && !caps.install_cli_available {
            return CommandAvailability {
                enabled: false,
                reason: Some(CommandUnavailableReason::InstallCliAlreadyInstalled),
            };
        }

        if self.is_ai_only() && !caps.ai_features_enabled {
            return CommandAvailability {
                enabled: false,
                reason: Some(CommandUnavailableReason::AiFeaturesDisabled),
            };
        }

        CommandAvailability {
            enabled: true,
            reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandCapabilities, CommandUnavailableReason};
    use crate::CommandId;

    #[test]
    fn command_availability_reports_requires_tmux_when_runtime_disabled() {
        let caps = CommandCapabilities {
            tmux_runtime_active: false,
            install_cli_available: true,
            ai_features_enabled: true,
        };
        let availability = CommandId::SplitPaneVertical.availability(caps);
        assert!(!availability.enabled);
        assert_eq!(
            availability.reason,
            Some(CommandUnavailableReason::RequiresTmuxRuntime)
        );
    }

    #[test]
    fn command_availability_reports_install_cli_when_already_installed() {
        let caps = CommandCapabilities {
            tmux_runtime_active: true,
            install_cli_available: false,
            ai_features_enabled: true,
        };
        let availability = CommandId::InstallCli.availability(caps);
        assert!(!availability.enabled);
        assert_eq!(
            availability.reason,
            Some(CommandUnavailableReason::InstallCliAlreadyInstalled)
        );
    }

    #[test]
    fn command_availability_reports_ai_features_disabled() {
        let caps = CommandCapabilities {
            tmux_runtime_active: true,
            install_cli_available: true,
            ai_features_enabled: false,
        };
        let availability = CommandId::ToggleAiInput.availability(caps);
        assert!(!availability.enabled);
        assert_eq!(
            availability.reason,
            Some(CommandUnavailableReason::AiFeaturesDisabled)
        );

        let availability = CommandId::ToggleChatSidebar.availability(caps);
        assert!(!availability.enabled);
        assert_eq!(
            availability.reason,
            Some(CommandUnavailableReason::AiFeaturesDisabled)
        );
    }

    #[test]
    fn ai_commands_available_when_ai_features_enabled() {
        let caps = CommandCapabilities {
            tmux_runtime_active: true,
            install_cli_available: true,
            ai_features_enabled: true,
        };
        let availability = CommandId::ToggleAiInput.availability(caps);
        assert!(availability.enabled);
        assert_eq!(availability.reason, None);

        let availability = CommandId::ToggleChatSidebar.availability(caps);
        assert!(availability.enabled);
        assert_eq!(availability.reason, None);
    }
}
