use termy_auto_update::UpdateState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateBannerTone {
    Info,
    Success,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateBannerAction {
    Install,
    CompleteInstall,
    Restart,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateButtonStyle {
    Primary,
    Secondary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateBannerButton {
    pub label: &'static str,
    pub action: UpdateBannerAction,
    pub style: UpdateButtonStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateBannerModel {
    pub badge: &'static str,
    pub message: String,
    pub detail: Option<String>,
    pub progress_percent: Option<u8>,
    pub tone: UpdateBannerTone,
    pub buttons: Vec<UpdateBannerButton>,
}

impl UpdateBannerModel {
    pub fn from_state(state: &UpdateState) -> Option<Self> {
        match state {
            UpdateState::Available { version, .. } => Some(Self {
                badge: "Available",
                message: format!("Version {version} is ready"),
                detail: Some("A new update is available for install.".to_string()),
                progress_percent: None,
                tone: UpdateBannerTone::Info,
                buttons: vec![
                    UpdateBannerButton {
                        label: "Install",
                        action: UpdateBannerAction::Install,
                        style: UpdateButtonStyle::Primary,
                    },
                    UpdateBannerButton {
                        label: "Dismiss",
                        action: UpdateBannerAction::Dismiss,
                        style: UpdateButtonStyle::Secondary,
                    },
                ],
            }),
            UpdateState::Downloading {
                version,
                downloaded,
                total,
            } => {
                let progress_percent = if *total > 0 {
                    Some(((*downloaded as f64 / *total as f64) * 100.0).clamp(0.0, 100.0) as u8)
                } else {
                    None
                };

                let detail = if let Some(percent) = progress_percent {
                    Some(format!("Downloading {percent}%"))
                } else {
                    Some(format!("Downloaded {} KB", *downloaded / 1024))
                };

                Some(Self {
                    badge: "Downloading",
                    message: format!("Fetching version {version}"),
                    detail,
                    progress_percent,
                    tone: UpdateBannerTone::Info,
                    buttons: vec![],
                })
            }
            UpdateState::Downloaded { version, .. } => Some(Self {
                badge: "Ready",
                message: format!("Version {version} is downloaded"),
                detail: Some("Install now to finish the update.".to_string()),
                progress_percent: Some(100),
                tone: UpdateBannerTone::Success,
                buttons: vec![UpdateBannerButton {
                    label: "Install Now",
                    action: UpdateBannerAction::CompleteInstall,
                    style: UpdateButtonStyle::Primary,
                }],
            }),
            UpdateState::Installing { version } => Some(Self {
                badge: "Installing",
                message: format!("Installing version {version}"),
                detail: Some("Finishing system update steps...".to_string()),
                progress_percent: None,
                tone: UpdateBannerTone::Info,
                buttons: vec![],
            }),
            UpdateState::Installed { version } => Some(Self {
                badge: "Installed",
                message: format!("Version {version} installed"),
                detail: Some("Restart Termy to apply the update.".to_string()),
                progress_percent: None,
                tone: UpdateBannerTone::Success,
                buttons: vec![
                    UpdateBannerButton {
                        label: "Restart",
                        action: UpdateBannerAction::Restart,
                        style: UpdateButtonStyle::Primary,
                    },
                    UpdateBannerButton {
                        label: "Dismiss",
                        action: UpdateBannerAction::Dismiss,
                        style: UpdateButtonStyle::Secondary,
                    },
                ],
            }),
            UpdateState::Error(message) => Some(Self {
                badge: "Error",
                message: "Update failed".to_string(),
                detail: Some(message.clone()),
                progress_percent: None,
                tone: UpdateBannerTone::Error,
                buttons: vec![UpdateBannerButton {
                    label: "Dismiss",
                    action: UpdateBannerAction::Dismiss,
                    style: UpdateButtonStyle::Secondary,
                }],
            }),
            UpdateState::Idle | UpdateState::Checking | UpdateState::UpToDate => None,
        }
    }
}
