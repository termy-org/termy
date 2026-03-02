use super::*;

impl TerminalView {
    pub(super) fn sync_update_toasts(&mut self, state: Option<&UpdateState>) {
        let changed = self.last_notified_update_state.as_ref() != state;
        if !changed {
            return;
        }

        self.last_notified_update_state = state.cloned();

        // Helper to update the loading toast or create a new one
        let update_or_create =
            |toast_id: &mut Option<u64>, kind: termy_toast::ToastKind, msg: String| {
                if let Some(id) = *toast_id {
                    termy_toast::update_toast(id, kind, msg);
                } else {
                    let id = termy_toast::enqueue_toast_with_id(kind, msg, None);
                    *toast_id = Some(id);
                }
            };

        match state {
            Some(UpdateState::Available { version, .. }) => {
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Info,
                    format!("Update v{} available", version),
                );
            }
            Some(UpdateState::Downloaded { version, .. }) => {
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Success,
                    format!("v{} ready to install", version),
                );
            }
            Some(UpdateState::Installing { version }) => {
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Loading,
                    format!("Installing v{}", version),
                );
            }
            Some(UpdateState::Installed { version }) => {
                #[cfg(target_os = "macos")]
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Success,
                    format!("v{} installed \u{2014} reopen from /Applications", version),
                );
                #[cfg(target_os = "windows")]
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Success,
                    format!("v{} installed \u{2014} restart to apply", version),
                );
                #[cfg(target_os = "linux")]
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Success,
                    format!(
                        "v{} installed to ~/.local/bin \u{2014} restart to apply",
                        version
                    ),
                );
                #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Success,
                    format!("v{} installed \u{2014} restart to apply", version),
                );
            }
            Some(UpdateState::Error(message)) => {
                update_or_create(
                    &mut self.update_check_toast_id,
                    termy_toast::ToastKind::Error,
                    format!("Update failed: {}", message),
                );
            }
            Some(UpdateState::UpToDate) => {
                if let Some(id) = self.update_check_toast_id.take() {
                    termy_toast::dismiss_toast(id);
                }
            }
            _ => {}
        }
    }
}
