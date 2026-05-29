#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChromeContrastProfile {
    pub(crate) stroke_mix: f32,
    pub(crate) surface_alpha_scale: f32,
    pub(crate) neutral_border_scale: f32,
    pub(crate) accent_alpha_scale: f32,
    pub(crate) panel_alpha_bonus: f32,
}

impl ChromeContrastProfile {
    pub(crate) fn from_enabled(enabled: bool) -> Self {
        if enabled {
            Self {
                stroke_mix: 0.18,
                surface_alpha_scale: 1.45,
                neutral_border_scale: 1.60,
                accent_alpha_scale: 1.20,
                panel_alpha_bonus: 0.08,
            }
        } else {
            Self {
                stroke_mix: 0.12,
                surface_alpha_scale: 1.00,
                neutral_border_scale: 1.00,
                accent_alpha_scale: 1.00,
                panel_alpha_bonus: 0.00,
            }
        }
    }

    pub(crate) fn surface_alpha(self, base_alpha: f32) -> f32 {
        (base_alpha * self.surface_alpha_scale).clamp(0.0, 1.0)
    }

    pub(crate) fn neutral_border_alpha(self, base_alpha: f32) -> f32 {
        (base_alpha * self.neutral_border_scale).clamp(0.0, 1.0)
    }

    pub(crate) fn accent_alpha(self, base_alpha: f32) -> f32 {
        (base_alpha * self.accent_alpha_scale).clamp(0.0, 1.0)
    }

    pub(crate) fn panel_surface_alpha(self, base_alpha: f32) -> f32 {
        (self.surface_alpha(base_alpha) + self.panel_alpha_bonus).clamp(0.0, 1.0)
    }

    pub(crate) fn panel_neutral_alpha(self, base_alpha: f32) -> f32 {
        (self.neutral_border_alpha(base_alpha) + self.panel_alpha_bonus).clamp(0.0, 1.0)
    }

    pub(crate) fn panel_accent_alpha(self, base_alpha: f32) -> f32 {
        (self.accent_alpha(base_alpha) + self.panel_alpha_bonus).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_profile_matches_current_baseline() {
        let profile = ChromeContrastProfile::from_enabled(false);
        assert_eq!(profile.stroke_mix, 0.12);
        assert_eq!(profile.surface_alpha(0.24), 0.24);
        assert_eq!(profile.neutral_border_alpha(0.24), 0.24);
        assert_eq!(profile.accent_alpha(0.24), 0.24);
        assert_eq!(profile.panel_surface_alpha(0.24), 0.24);
    }

    #[test]
    fn enabled_profile_increases_emphasis_over_baseline() {
        let normal = ChromeContrastProfile::from_enabled(false);
        let enabled = ChromeContrastProfile::from_enabled(true);
        assert!(enabled.stroke_mix > normal.stroke_mix);
        assert!(enabled.surface_alpha(0.2) > normal.surface_alpha(0.2));
        assert!(enabled.neutral_border_alpha(0.2) > normal.neutral_border_alpha(0.2));
        assert!(enabled.panel_surface_alpha(0.2) > normal.panel_surface_alpha(0.2));
    }
}
