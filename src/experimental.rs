use crate::settings_view::SettingsSection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ExperimentalFeature {
    pub(crate) crate_name: &'static str,
    pub(crate) title: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) details: &'static str,
    pub(crate) toggle_setting_key: Option<&'static str>,
    pub(crate) settings_section: Option<SettingsSection>,
}

const EXPERIMENTAL_FEATURES: &[ExperimentalFeature] =
    include!(concat!(env!("OUT_DIR"), "/experimental_features.rs"));

pub(crate) fn entries() -> &'static [ExperimentalFeature] {
    EXPERIMENTAL_FEATURES
}

pub(crate) fn has_entries() -> bool {
    !EXPERIMENTAL_FEATURES.is_empty()
}
