use super::super::*;
use super::chrome;

#[derive(Clone, Copy)]
pub(super) struct TabStripPalette {
    pub(super) tab_stroke_color: gpui::Rgba,
    pub(super) inactive_tab_bg: gpui::Rgba,
    pub(super) active_tab_bg: gpui::Rgba,
    pub(super) hovered_tab_bg: gpui::Rgba,
    pub(super) active_tab_text: gpui::Rgba,
    pub(super) inactive_tab_text: gpui::Rgba,
    pub(super) close_button_bg: gpui::Rgba,
    pub(super) close_button_border: gpui::Rgba,
    pub(super) close_button_hover_bg: gpui::Rgba,
    pub(super) close_button_hover_border: gpui::Rgba,
    pub(super) close_button_hover_text: gpui::Rgba,
    pub(super) switch_hint_bg: gpui::Rgba,
    pub(super) switch_hint_border: gpui::Rgba,
    pub(super) switch_hint_text: gpui::Rgba,
    pub(super) tab_drop_marker_color: gpui::Rgba,
    pub(super) tabbar_new_tab_bg: gpui::Rgba,
    pub(super) tabbar_new_tab_hover_bg: gpui::Rgba,
    pub(super) tabbar_new_tab_border: gpui::Rgba,
    pub(super) tabbar_new_tab_hover_border: gpui::Rgba,
    pub(super) tabbar_new_tab_text: gpui::Rgba,
    pub(super) tabbar_new_tab_hover_text: gpui::Rgba,
}

impl TerminalView {
    pub(super) fn resolve_tab_strip_palette(
        &self,
        colors: &TerminalColors,
        tabbar_bg: gpui::Rgba,
    ) -> TabStripPalette {
        let tab_stroke_color = chrome::resolve_tab_stroke_color(
            tabbar_bg,
            colors.foreground,
            TAB_STROKE_FOREGROUND_MIX,
        );
        let mut inactive_tab_bg = colors.foreground;
        inactive_tab_bg.a = self.scaled_chrome_alpha(0.10);
        let mut active_tab_bg = tabbar_bg;
        active_tab_bg.a = 0.0;
        let mut hovered_tab_bg = colors.foreground;
        hovered_tab_bg.a = self.scaled_chrome_alpha(0.13);
        let mut active_tab_text = colors.foreground;
        active_tab_text.a = 0.95;
        let mut inactive_tab_text = colors.foreground;
        inactive_tab_text.a = 0.7;
        let mut close_button_bg = colors.foreground;
        close_button_bg.a = self.scaled_chrome_alpha(0.07);
        let mut close_button_border = colors.foreground;
        close_button_border.a = self.scaled_chrome_alpha(0.14);
        let mut close_button_hover_bg = colors.foreground;
        close_button_hover_bg.a = self.scaled_chrome_alpha(0.16);
        let mut close_button_hover_border = colors.cursor;
        close_button_hover_border.a = self.scaled_chrome_alpha(0.4);
        let mut close_button_hover_text = colors.foreground;
        close_button_hover_text.a = 0.98;
        let now = Instant::now();
        let hint_progress = self.tab_switch_hint_progress(now);
        let mut switch_hint_bg = colors.cursor;
        switch_hint_bg.a = self.scaled_chrome_alpha(0.18 * hint_progress);
        let mut switch_hint_border = colors.cursor;
        switch_hint_border.a = self.scaled_chrome_alpha(0.52 * hint_progress);
        let mut switch_hint_text = colors.foreground;
        switch_hint_text.a = (0.99 * hint_progress).clamp(0.0, 1.0);
        let mut tab_drop_marker_color = colors.cursor;
        tab_drop_marker_color.a = self.scaled_chrome_alpha(0.95);
        let mut tabbar_new_tab_bg = colors.foreground;
        tabbar_new_tab_bg.a = self.scaled_chrome_alpha(0.11);
        let mut tabbar_new_tab_hover_bg = colors.foreground;
        tabbar_new_tab_hover_bg.a = self.scaled_chrome_alpha(0.2);
        let mut tabbar_new_tab_border = colors.foreground;
        tabbar_new_tab_border.a = self.scaled_chrome_alpha(0.24);
        let mut tabbar_new_tab_hover_border = colors.cursor;
        tabbar_new_tab_hover_border.a = self.scaled_chrome_alpha(0.76);
        let mut tabbar_new_tab_text = colors.foreground;
        tabbar_new_tab_text.a = 0.9;
        let mut tabbar_new_tab_hover_text = colors.cursor;
        tabbar_new_tab_hover_text.a = 0.98;

        TabStripPalette {
            tab_stroke_color,
            inactive_tab_bg,
            active_tab_bg,
            hovered_tab_bg,
            active_tab_text,
            inactive_tab_text,
            close_button_bg,
            close_button_border,
            close_button_hover_bg,
            close_button_hover_border,
            close_button_hover_text,
            switch_hint_bg,
            switch_hint_border,
            switch_hint_text,
            tab_drop_marker_color,
            tabbar_new_tab_bg,
            tabbar_new_tab_hover_bg,
            tabbar_new_tab_border,
            tabbar_new_tab_hover_border,
            tabbar_new_tab_text,
            tabbar_new_tab_hover_text,
        }
    }
}
