use super::super::{
    COMMAND_PALETTE_INPUT_BG_ALPHA, COMMAND_PALETTE_INPUT_SELECTION_ALPHA,
    COMMAND_PALETTE_INPUT_SOLID_ALPHA, COMMAND_PALETTE_PANEL_BG_ALPHA,
    COMMAND_PALETTE_PANEL_SOLID_ALPHA, COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA,
    COMMAND_PALETTE_SCROLLBAR_THUMB_ALPHA, COMMAND_PALETTE_SCROLLBAR_TRACK_ALPHA,
    COMMAND_PALETTE_SHORTCUT_BG_ALPHA, COMMAND_PALETTE_SHORTCUT_TEXT_ALPHA,
    OVERLAY_MUTED_TEXT_ALPHA, OVERLAY_PRIMARY_TEXT_ALPHA, TerminalView,
    resolve_chrome_stroke_color,
};

pub(in super::super) const COMMAND_PALETTE_PANEL_RADIUS: f32 = 0.0;
pub(in super::super) const COMMAND_PALETTE_INPUT_RADIUS: f32 = 0.0;
pub(super) const COMMAND_PALETTE_ROW_RADIUS: f32 = 0.0;
pub(super) const COMMAND_PALETTE_SHORTCUT_RADIUS: f32 = 0.0;

#[derive(Clone, Copy)]
pub(in super::super) struct CommandPaletteStyle {
    pub(in super::super) panel_bg: gpui::Rgba,
    pub(in super::super) panel_border: gpui::Rgba,
    pub(in super::super) primary_text: gpui::Rgba,
    pub(in super::super) muted_text: gpui::Rgba,
    pub(in super::super) input_bg: gpui::Rgba,
    pub(in super::super) input_selection: gpui::Rgba,
    pub(super) selected_bg: gpui::Rgba,
    pub(super) selected_border: gpui::Rgba,
    pub(super) shortcut_bg: gpui::Rgba,
    pub(super) shortcut_border: gpui::Rgba,
    pub(super) shortcut_text: gpui::Rgba,
    pub(super) scrollbar_track: gpui::Rgba,
    pub(super) scrollbar_thumb: gpui::Rgba,
}

pub(super) fn command_palette_border_color(
    chrome_surface_bg: gpui::Rgba,
    foreground: gpui::Rgba,
    stroke_mix: f32,
) -> gpui::Rgba {
    resolve_chrome_stroke_color(chrome_surface_bg, foreground, stroke_mix)
}

impl CommandPaletteStyle {
    pub(in super::super) fn resolve(view: &TerminalView) -> Self {
        let overlay_style = view.overlay_style();
        let panel_bg = overlay_style.chrome_panel_background_with_floor(
            COMMAND_PALETTE_PANEL_BG_ALPHA,
            COMMAND_PALETTE_PANEL_SOLID_ALPHA,
        );

        let mut chrome_surface_bg = view.colors.background;
        chrome_surface_bg.a = view.scaled_background_alpha(chrome_surface_bg.a);
        let panel_border = command_palette_border_color(
            chrome_surface_bg,
            view.colors.foreground,
            view.chrome_contrast_profile().stroke_mix,
        );

        let selected_bg = overlay_style.chrome_panel_cursor(COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA);
        let primary_text = overlay_style.panel_foreground(OVERLAY_PRIMARY_TEXT_ALPHA);
        let muted_text = overlay_style.panel_foreground(OVERLAY_MUTED_TEXT_ALPHA);
        let input_bg = overlay_style.chrome_panel_background_with_floor(
            COMMAND_PALETTE_INPUT_BG_ALPHA,
            COMMAND_PALETTE_INPUT_SOLID_ALPHA,
        );
        let input_selection =
            overlay_style.chrome_panel_cursor(COMMAND_PALETTE_INPUT_SELECTION_ALPHA);
        let shortcut_bg = overlay_style.chrome_panel_cursor(COMMAND_PALETTE_SHORTCUT_BG_ALPHA);
        let shortcut_text = overlay_style.panel_foreground(COMMAND_PALETTE_SHORTCUT_TEXT_ALPHA);
        let scrollbar_track =
            view.scrollbar_color(overlay_style, COMMAND_PALETTE_SCROLLBAR_TRACK_ALPHA);
        let scrollbar_thumb =
            view.scrollbar_color(overlay_style, COMMAND_PALETTE_SCROLLBAR_THUMB_ALPHA);

        Self {
            panel_bg,
            panel_border,
            primary_text,
            muted_text,
            input_bg,
            input_selection,
            selected_bg,
            selected_border: panel_border,
            shortcut_bg,
            shortcut_border: panel_border,
            shortcut_text,
            scrollbar_track,
            scrollbar_thumb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharp_geometry_defaults_to_square_edges() {
        assert_eq!(COMMAND_PALETTE_PANEL_RADIUS, 0.0);
        assert_eq!(COMMAND_PALETTE_INPUT_RADIUS, 0.0);
        assert_eq!(COMMAND_PALETTE_ROW_RADIUS, 0.0);
        assert_eq!(COMMAND_PALETTE_SHORTCUT_RADIUS, 0.0);
    }

    #[test]
    fn command_palette_border_matches_shared_chrome_stroke_derivation() {
        let chrome_surface_bg = gpui::Rgba {
            r: 0.02,
            g: 0.05,
            b: 0.12,
            a: 0.9,
        };
        let foreground = gpui::Rgba {
            r: 0.8,
            g: 0.88,
            b: 0.93,
            a: 1.0,
        };

        let stroke_mix = crate::chrome_style::ChromeContrastProfile::from_enabled(false).stroke_mix;
        let border = command_palette_border_color(chrome_surface_bg, foreground, stroke_mix);
        let tab_stroke = resolve_chrome_stroke_color(chrome_surface_bg, foreground, stroke_mix);

        assert_eq!(border, tab_stroke);
    }
}
