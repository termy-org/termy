use super::*;

impl SettingsWindow {
    pub(super) fn background_opacity_slider_width() -> f32 {
        let fixed = SETTINGS_SLIDER_VALUE_WIDTH + (NUMERIC_STEP_BUTTON_SIZE * 2.0);
        let gaps = SETTINGS_OPACITY_CONTROL_GAP * 3.0;
        (SETTINGS_CONTROL_WIDTH - (SETTINGS_CONTROL_INNER_PADDING * 2.0) - fixed - gaps).max(80.0)
    }

    pub(super) fn quantize_background_opacity_ratio(ratio: f32) -> f32 {
        let step = SETTINGS_OPACITY_STEP_RATIO;
        ((ratio.clamp(0.0, 1.0) / step).round() * step).clamp(0.0, 1.0)
    }

    pub(super) fn background_opacity_ratio_from_local_x(local_x: f32, slider_width: f32) -> f32 {
        (local_x / slider_width.max(1.0)).clamp(0.0, 1.0)
    }

    pub(super) fn background_opacity_local_x_from_window_x(
        window_x: f32,
        slider_left: f32,
        slider_width: f32,
    ) -> f32 {
        (window_x - slider_left).clamp(0.0, slider_width.max(1.0))
    }

    pub(super) fn set_background_opacity_preview(&mut self, ratio: f32) -> bool {
        let ratio = Self::quantize_background_opacity_ratio(ratio);
        if (self.effective_background_opacity() - ratio).abs() < f32::EPSILON {
            return false;
        }
        let preview = config::BackgroundOpacityPreview {
            owner_id: self.background_opacity_preview_owner_id,
            opacity: ratio,
        };
        self.preview_background_opacity = Some(preview);
        config::publish_background_opacity_preview(Some(preview));
        true
    }

    pub(super) fn persist_background_opacity(&mut self, ratio: f32) -> Result<(), String> {
        let ratio = Self::quantize_background_opacity_ratio(ratio);
        let previous = self.config.background_opacity;
        self.config.background_opacity = ratio;
        if let Err(error) =
            config::set_root_setting(RootSettingId::BackgroundOpacity, &format!("{ratio:.3}"))
        {
            self.config.background_opacity = previous;
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn background_opacity_slider_local_x(&self, window_x: f32) -> Option<f32> {
        let bounds = self.background_opacity_slider_bounds?;
        let slider_left: f32 = bounds.left().into();
        let slider_width: f32 = bounds.size.width.into();
        Some(Self::background_opacity_local_x_from_window_x(
            window_x,
            slider_left,
            slider_width,
        ))
    }

    pub(super) fn begin_background_opacity_drag(&mut self, local_x: f32) {
        self.background_opacity_drag_state = Some(BackgroundOpacityDragState {
            start_local_x: local_x,
            start_ratio: self.effective_background_opacity(),
        });
    }

    pub(super) fn update_background_opacity_drag(
        &mut self,
        window_x: f32,
        slider_width: f32,
    ) -> bool {
        let Some(drag_state) = self.background_opacity_drag_state else {
            return false;
        };
        let Some(local_x) = self.background_opacity_slider_local_x(window_x) else {
            return false;
        };
        let delta_ratio = (local_x - drag_state.start_local_x) / slider_width.max(1.0);
        self.set_background_opacity_preview(drag_state.start_ratio + delta_ratio)
    }

    pub(super) fn set_background_opacity_from_slider_position(
        &mut self,
        window_x: f32,
        slider_width: f32,
    ) -> bool {
        let Some(local_x) = self.background_opacity_slider_local_x(window_x) else {
            return false;
        };
        let ratio = Self::background_opacity_ratio_from_local_x(local_x, slider_width);
        self.set_background_opacity_preview(ratio)
    }

    pub(super) fn finish_background_opacity_drag(&mut self) -> Result<bool, String> {
        let Some(_drag_state) = self.background_opacity_drag_state.take() else {
            return Ok(false);
        };
        let saved_ratio = self.config.background_opacity;
        let ratio = self.effective_background_opacity();
        if (ratio - saved_ratio).abs() < f32::EPSILON {
            self.clear_background_opacity_preview();
            return Ok(false);
        }
        if let Err(error) = self.persist_background_opacity(ratio) {
            self.clear_background_opacity_preview();
            return Err(error);
        }
        self.clear_background_opacity_preview();
        Ok((ratio - saved_ratio).abs() >= f32::EPSILON)
    }

    pub(super) fn step_background_opacity(&mut self, delta: i32) -> Result<bool, String> {
        let next = self.config.background_opacity + (delta as f32 * SETTINGS_OPACITY_STEP_RATIO);
        let next = Self::quantize_background_opacity_ratio(next);
        if (self.config.background_opacity - next).abs() < f32::EPSILON {
            return Ok(false);
        }
        self.clear_background_opacity_preview();
        self.persist_background_opacity(next)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::SettingsWindow;

    #[test]
    fn background_opacity_ratio_from_local_x_clamps() {
        assert_eq!(
            SettingsWindow::background_opacity_ratio_from_local_x(-10.0, 120.0),
            0.0
        );
        assert_eq!(
            SettingsWindow::background_opacity_ratio_from_local_x(60.0, 120.0),
            0.5
        );
        assert_eq!(
            SettingsWindow::background_opacity_ratio_from_local_x(200.0, 120.0),
            1.0
        );
    }

    #[test]
    fn background_opacity_local_x_from_window_x_accounts_for_slider_offset() {
        assert_eq!(
            SettingsWindow::background_opacity_local_x_from_window_x(260.0, 200.0, 120.0),
            60.0
        );
        assert_eq!(
            SettingsWindow::background_opacity_local_x_from_window_x(500.0, 200.0, 120.0),
            120.0
        );
    }

    #[test]
    fn drag_delta_uses_slider_local_coordinates() {
        let slider_width = 120.0;
        let slider_left = 200.0;
        let start_window_x = 260.0;
        let current_window_x = 296.0;
        let start_local_x = SettingsWindow::background_opacity_local_x_from_window_x(
            start_window_x,
            slider_left,
            slider_width,
        );
        let current_local_x = SettingsWindow::background_opacity_local_x_from_window_x(
            current_window_x,
            slider_left,
            slider_width,
        );

        let delta_ratio = (current_local_x - start_local_x) / slider_width;
        let next_ratio = SettingsWindow::quantize_background_opacity_ratio(0.5 + delta_ratio);

        assert_eq!(start_local_x, 60.0);
        assert_eq!(current_local_x, 96.0);
        assert_eq!(next_ratio, 0.8);
    }
}
