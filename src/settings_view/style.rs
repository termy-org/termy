use super::*;

impl SettingsWindow {
    pub(super) fn background_opacity_factor(&self) -> f32 {
        self.effective_background_opacity()
    }

    pub(super) fn scaled_background_alpha(&self, base_alpha: f32) -> f32 {
        (base_alpha * self.background_opacity_factor()).clamp(0.0, 1.0)
    }

    pub(super) fn adaptive_panel_alpha(&self, base_alpha: f32) -> f32 {
        let floor = base_alpha * SETTINGS_OVERLAY_PANEL_ALPHA_FLOOR_RATIO;
        self.scaled_background_alpha(base_alpha)
            .max(floor)
            .clamp(0.0, 1.0)
    }

    pub(super) fn sync_window_background_appearance(&mut self, window: &mut Window) {
        let mut preview_config = self.config.clone();
        preview_config.background_opacity = self.effective_background_opacity();
        let appearance =
            crate::terminal_view::initial_window_background_appearance(&preview_config);
        if self.last_window_background_appearance != Some(appearance) {
            window.set_background_appearance(appearance);
            self.last_window_background_appearance = Some(appearance);
        }
    }

    pub(super) fn bg_primary(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.scaled_background_alpha(c.a);
        c
    }

    pub(super) fn bg_secondary(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.7);
        c
    }

    pub(super) fn bg_card(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.5);
        c
    }

    pub(super) fn bg_input(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.36);
        c
    }

    pub(super) fn bg_hover(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.1;
        c
    }

    pub(super) fn bg_active(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.15;
        c
    }

    pub(super) fn text_primary(&self) -> Rgba {
        self.colors.foreground
    }

    pub(super) fn text_secondary(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.82;
        c
    }

    pub(super) fn text_muted(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.68;
        c
    }

    pub(super) fn border_color(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.24;
        c
    }

    pub(super) fn accent(&self) -> Rgba {
        self.colors.cursor
    }

    pub(super) fn accent_with_alpha(&self, alpha: f32) -> Rgba {
        let mut c = self.colors.cursor;
        c.a = alpha;
        c
    }

    pub(super) fn settings_scrollbar_style(&self) -> ScrollbarPaintStyle {
        let mut track = self.colors.foreground;
        track.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_TRACK_ALPHA);

        let mut thumb = self.colors.foreground;
        thumb.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_THUMB_ALPHA);

        let mut active_thumb = self.colors.foreground;
        active_thumb.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_THUMB_ACTIVE_ALPHA);

        ScrollbarPaintStyle {
            width: SETTINGS_SCROLLBAR_WIDTH,
            track_radius: 0.0,
            thumb_radius: 0.0,
            thumb_inset: 0.0,
            marker_inset: 0.0,
            marker_radius: 0.0,
            track_color: track,
            thumb_color: thumb,
            active_thumb_color: active_thumb,
            marker_color: None,
            current_marker_color: None,
        }
    }

    pub(super) fn settings_scrollbar_metrics(
        &self,
        window: &Window,
    ) -> Option<ui_scrollbar::ScrollbarMetrics> {
        let viewport_height: f32 = window.viewport_size().height.into();
        let max_offset: f32 = self.content_scroll_handle.max_offset().height.into();
        let offset_y: f32 = self.content_scroll_handle.offset().y.into();
        let offset = (-offset_y).max(0.0);
        let range = ScrollbarRange {
            offset,
            max_offset,
            viewport_extent: viewport_height,
            track_extent: viewport_height,
        };

        ui_scrollbar::compute_metrics(range, SETTINGS_SCROLLBAR_MIN_THUMB_HEIGHT)
    }

    pub(super) fn request_scrollbar_refresh_frames(
        &mut self,
        frames_remaining: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if frames_remaining == 0 {
            return;
        }

        let this = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = this.update(cx, |view, cx| {
                cx.notify();
                view.request_scrollbar_refresh_frames(frames_remaining - 1, window, cx);
            });
        });
    }

    pub(super) fn srgb_to_linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    pub(super) fn composite_over(fg: Rgba, bg: Rgba) -> Rgba {
        let fg_alpha = fg.a.clamp(0.0, 1.0);
        Rgba {
            r: (fg_alpha * fg.r + (1.0 - fg_alpha) * bg.r).clamp(0.0, 1.0),
            g: (fg_alpha * fg.g + (1.0 - fg_alpha) * bg.g).clamp(0.0, 1.0),
            b: (fg_alpha * fg.b + (1.0 - fg_alpha) * bg.b).clamp(0.0, 1.0),
            a: 1.0,
        }
    }

    pub(super) fn relative_luminance(color: Rgba, backdrop: Rgba) -> f32 {
        let composited = Self::composite_over(color, backdrop);
        let r = Self::srgb_to_linear(composited.r);
        let g = Self::srgb_to_linear(composited.g);
        let b = Self::srgb_to_linear(composited.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    pub(super) fn contrast_ratio(a: Rgba, b: Rgba, backdrop: Rgba) -> f32 {
        let l1 = Self::relative_luminance(a, backdrop);
        let l2 = Self::relative_luminance(b, backdrop);
        let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    pub(super) fn contrasting_text_for_fill(&self, fill: Rgba, backdrop: Rgba) -> Rgba {
        let mut primary = self.text_primary();
        primary.a = 1.0;
        let mut dark = self.bg_primary();
        dark.a = 1.0;
        let mut backdrop = backdrop;
        backdrop.a = 1.0;
        let composited_fill = Self::composite_over(fill, backdrop);

        if Self::contrast_ratio(primary, composited_fill, backdrop)
            >= Self::contrast_ratio(dark, composited_fill, backdrop)
        {
            primary
        } else {
            dark
        }
    }
}
