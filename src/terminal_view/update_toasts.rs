use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdateToastEffect {
    None,
    DismissProgressToast,
    Enqueue {
        kind: termy_toast::ToastKind,
        message: String,
    },
    StartOrUpdateProgress {
        message: String,
    },
    FinishProgressOrEnqueue {
        kind: termy_toast::ToastKind,
        message: String,
    },
}

fn update_toast_effect(state: Option<&UpdateState>) -> UpdateToastEffect {
    match state {
        Some(UpdateState::Available { version, .. }) => UpdateToastEffect::Enqueue {
            kind: termy_toast::ToastKind::Info,
            message: format!("Update v{} available", version),
        },
        Some(UpdateState::Downloaded { version, .. }) => UpdateToastEffect::Enqueue {
            kind: termy_toast::ToastKind::Success,
            message: format!("v{} ready to install", version),
        },
        Some(UpdateState::Installing { version }) => UpdateToastEffect::StartOrUpdateProgress {
            message: format!("Installing v{}", version),
        },
        Some(UpdateState::Installed { version }) => UpdateToastEffect::FinishProgressOrEnqueue {
            kind: termy_toast::ToastKind::Success,
            message: installed_update_toast_message(version),
        },
        Some(UpdateState::Error(message)) => UpdateToastEffect::FinishProgressOrEnqueue {
            kind: termy_toast::ToastKind::Error,
            message: format!("Update failed: {}", message),
        },
        Some(UpdateState::UpToDate) => UpdateToastEffect::DismissProgressToast,
        _ => UpdateToastEffect::None,
    }
}

fn installed_update_toast_message(version: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("v{} installed \u{2014} reopen from /Applications", version)
    }
    #[cfg(target_os = "windows")]
    {
        format!("v{} installed \u{2014} restart to apply", version)
    }
    #[cfg(target_os = "linux")]
    {
        format!(
            "v{} installed to ~/.local/bin \u{2014} restart to apply",
            version
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        format!("v{} installed \u{2014} restart to apply", version)
    }
}

impl TerminalView {
    pub(super) fn sync_update_toasts(&mut self, state: Option<&UpdateState>) {
        let changed = self.last_notified_update_state.as_ref() != state;
        if !changed {
            return;
        }

        self.last_notified_update_state = state.cloned();

        match update_toast_effect(state) {
            UpdateToastEffect::None => {}
            UpdateToastEffect::DismissProgressToast => {
                if let Some(id) = self.update_check_toast_id.take() {
                    termy_toast::dismiss_toast(id);
                }
            }
            UpdateToastEffect::Enqueue { kind, message } => {
                if let Some(id) = self.update_check_toast_id.take() {
                    termy_toast::dismiss_toast(id);
                }
                termy_toast::enqueue_toast(kind, message, None);
            }
            UpdateToastEffect::StartOrUpdateProgress { message } => {
                if let Some(id) = self.update_check_toast_id {
                    termy_toast::update_toast(id, termy_toast::ToastKind::Loading, message);
                } else {
                    self.update_check_toast_id = Some(termy_toast::loading(message));
                }
            }
            UpdateToastEffect::FinishProgressOrEnqueue { kind, message } => {
                if let Some(id) = self.update_check_toast_id.take() {
                    termy_toast::update_toast(id, kind, message);
                } else {
                    termy_toast::enqueue_toast(kind, message, None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn completed_update_states_enqueue_independent_stack_entries() {
        assert_eq!(
            update_toast_effect(Some(&UpdateState::Available {
                version: "0.1.79".to_string(),
                url: "https://example.com".to_string(),
                extension: "dmg".to_string(),
            })),
            UpdateToastEffect::Enqueue {
                kind: termy_toast::ToastKind::Info,
                message: "Update v0.1.79 available".to_string(),
            }
        );
        assert_eq!(
            update_toast_effect(Some(&UpdateState::Downloaded {
                version: "0.1.79".to_string(),
                installer_path: PathBuf::from("/tmp/termy"),
            })),
            UpdateToastEffect::Enqueue {
                kind: termy_toast::ToastKind::Success,
                message: "v0.1.79 ready to install".to_string(),
            }
        );
    }

    #[test]
    fn progress_update_states_keep_one_mutable_loading_toast() {
        assert_eq!(
            update_toast_effect(Some(&UpdateState::Installing {
                version: "0.1.79".to_string(),
            })),
            UpdateToastEffect::StartOrUpdateProgress {
                message: "Installing v0.1.79".to_string(),
            }
        );
        assert!(matches!(
            update_toast_effect(Some(&UpdateState::Installed {
                version: "0.1.79".to_string(),
            })),
            UpdateToastEffect::FinishProgressOrEnqueue {
                kind: termy_toast::ToastKind::Success,
                ..
            }
        ));
    }

    #[test]
    fn up_to_date_dismisses_progress_toast_without_adding_stack_entry() {
        assert_eq!(
            update_toast_effect(Some(&UpdateState::UpToDate)),
            UpdateToastEffect::DismissProgressToast
        );
        assert_eq!(update_toast_effect(None), UpdateToastEffect::None);
    }
}
