use super::{
    Context, EditableField, IntoElement, ParentElement, SettingsSection, SettingsWindow, Styled,
    div,
};

impl SettingsWindow {
    pub(super) fn render_colors_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = termy_config_core::color_setting_specs()
            .iter()
            .map(|spec| {
                let display = self
                    .custom_color_for_id(spec.id)
                    .map(|rgb| format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b))
                    .unwrap_or_else(|| "Theme default".to_string());
                self.render_editable_row(
                    spec.key,
                    EditableField::Color(spec.id),
                    spec.title,
                    spec.description,
                    display,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Colors",
                "Override individual terminal colors",
                SettingsSection::Colors,
                cx,
            ))
            .child(self.render_settings_group("OVERRIDES", rows))
    }
}
