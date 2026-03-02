#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AgentSidebarMode {
    open: bool,
    width: f32,
}

impl Default for AgentSidebarMode {
    fn default() -> Self {
        Self {
            open: false,
            width: termy_config_core::DEFAULT_CHAT_SIDEBAR_WIDTH,
        }
    }
}

impl AgentSidebarMode {
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn is_open(self) -> bool {
        self.open
    }

    pub fn width(self) -> f32 {
        self.width
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = clamp_sidebar_width(width);
    }

    pub fn active_width(self) -> f32 {
        if self.open { self.width } else { 0.0 }
    }
}

pub fn clamp_sidebar_width(width: f32) -> f32 {
    if !width.is_finite() {
        return termy_config_core::DEFAULT_CHAT_SIDEBAR_WIDTH;
    }
    width.clamp(
        termy_config_core::MIN_CHAT_SIDEBAR_WIDTH,
        termy_config_core::MAX_CHAT_SIDEBAR_WIDTH,
    )
}
