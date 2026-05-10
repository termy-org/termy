use super::*;

impl OnboardingWindow {
    pub(super) fn accent(&self) -> Rgba {
        self.colors.cursor
    }

    pub(super) fn accent_with_alpha(&self, alpha: f32) -> Rgba {
        let mut color = self.colors.cursor;
        color.a = alpha;
        color
    }

    pub(super) fn text_primary(&self) -> Rgba {
        self.colors.foreground
    }

    pub(super) fn text_muted(&self) -> Rgba {
        let mut color = self.colors.foreground;
        color.a = 0.62;
        color
    }

    pub(super) fn text_secondary(&self) -> Rgba {
        let mut color = self.colors.foreground;
        color.a = 0.82;
        color
    }

    pub(super) fn bg_card(&self) -> Rgba {
        let mut color = self.colors.foreground;
        color.a = 0.04;
        color
    }

    pub(super) fn bg_card_hover(&self) -> Rgba {
        let mut color = self.colors.foreground;
        color.a = 0.08;
        color
    }

    pub(super) fn border_color(&self) -> Rgba {
        let mut color = self.colors.foreground;
        color.a = 0.16;
        color
    }
}
