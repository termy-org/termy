use super::command_palette::style::{
    COMMAND_PALETTE_INPUT_RADIUS, COMMAND_PALETTE_PANEL_RADIUS, CommandPaletteStyle,
};
use super::inline_input::InlineInputAlignment;
use super::*;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};

const AI_INPUT_WIDTH: f32 = 640.0;
const AI_CONTEXT_LINES: i32 = 50;

impl TerminalView {
    pub(super) fn is_ai_input_open(&self) -> bool {
        self.ai_input_open
    }

    pub(super) fn ai_input(&self) -> &InlineInputState {
        &self.ai_input
    }

    pub(super) fn ai_input_mut(&mut self) -> &mut InlineInputState {
        &mut self.ai_input
    }

    pub(super) fn open_ai_input(&mut self, cx: &mut Context<Self>) {
        if self.ai_input_open {
            return;
        }

        let _ = self.close_terminal_context_menu(cx);

        // Close other overlays
        if self.is_command_palette_open() {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }
        if self.renaming_tab.is_some() {
            self.cancel_rename_tab(cx);
        }

        self.ai_input_open = true;
        self.ai_input.clear();
        self.reset_cursor_blink_phase();
        cx.notify();
    }

    pub(super) fn close_ai_input(&mut self, cx: &mut Context<Self>) {
        if !self.ai_input_open {
            return;
        }

        self.ai_input_open = false;
        self.ai_input.clear();
        cx.notify();
    }

    pub(super) fn handle_ai_input_key_down(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "escape" => {
                self.close_ai_input(cx);
            }
            "enter" => {
                let text = self.ai_input.text().trim().to_string();
                if !text.is_empty() {
                    self.submit_ai_input(text, cx);
                }
            }
            _ => {}
        }
    }

    fn submit_ai_input(&mut self, user_message: String, cx: &mut Context<Self>) {
        // Get the API key from config
        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "ai_input config load",
        );
        let provider = loaded.config.ai_provider;
        let api_key = match loaded.config.ai_provider {
            config::AiProvider::OpenAi => loaded.config.openai_api_key,
            config::AiProvider::Gemini => loaded.config.gemini_api_key,
        };
        let api_key = match api_key {
            Some(key) if !key.is_empty() => key,
            _ => {
                let provider_name = match provider {
                    config::AiProvider::OpenAi => "OpenAI",
                    config::AiProvider::Gemini => "Gemini",
                };
                termy_toast::error(format!(
                    "{provider_name} API key not configured. Set it in Settings > Advanced > AI."
                ));
                self.notify_overlay(cx);
                return;
            }
        };
        let model = loaded
            .config
            .openai_model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| match provider {
                config::AiProvider::OpenAi => termy_openai::DEFAULT_MODEL.to_string(),
                config::AiProvider::Gemini => termy_gemini::DEFAULT_MODEL.to_string(),
            });

        // Get terminal context
        let terminal_context = self.get_terminal_context_for_ai();

        // Close the input
        self.close_ai_input(cx);
        let loading_toast_id = termy_toast::loading(format!("Sending to AI ({model})..."));

        // Spawn async task to call OpenAI (using smol::unblock for blocking HTTP client)
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = smol::unblock(move || match provider {
                config::AiProvider::OpenAi => {
                    let client = termy_openai::OpenAiClient::new(api_key).with_model(model);
                    client
                        .message_with_terminal_context(&user_message, &terminal_context)
                        .map_err(|error| error.to_string())
                }
                config::AiProvider::Gemini => {
                    let client = termy_gemini::GeminiClient::new(api_key).with_model(model);
                    client
                        .message_with_terminal_context(&user_message, &terminal_context)
                        .map_err(|error| error.to_string())
                }
            })
            .await;

            // Dismiss loading toast
            termy_toast::dismiss_toast(loading_toast_id);

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(response) => {
                            // Put the response into the terminal
                            view.insert_ai_response(&response, cx);
                        }
                        Err(e) => {
                            termy_toast::error(format!("AI error: {}", e));
                            view.notify_overlay(cx);
                        }
                    }
                })
            });
        })
        .detach();
    }

    fn get_terminal_context_for_ai(&self) -> String {
        let Some(terminal) = self.active_terminal() else {
            return String::new();
        };

        let (_display_offset, history_size) = terminal.scroll_state();
        let size = terminal.size();
        let rows = size.rows as i32;

        // Get the last AI_CONTEXT_LINES lines (or whatever is available)
        let end_line = rows - 1;
        let available_history = history_size as i32;
        let start_line = (end_line - AI_CONTEXT_LINES + 1).max(-available_history);

        let mut lines = Vec::new();
        let _ = terminal.with_grid(|grid| {
            for line_idx in start_line..=end_line {
                if let Some(text) = extract_line_text_for_ai(grid, line_idx) {
                    let trimmed = text.trim_end();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }
            }
        });

        lines.join("\n")
    }

    fn insert_ai_response(&mut self, response: &str, cx: &mut Context<Self>) {
        // Strip markdown code blocks if present
        let cleaned = strip_markdown_code_block(response.trim());

        // Show in toast (truncated if too long)
        let display = if cleaned.len() > 200 {
            format!("{}...", &cleaned[..200])
        } else {
            cleaned.clone()
        };
        termy_toast::success(format!("AI: {}", display));

        // Write to terminal input buffer (as if user typed it)
        if let Some(tab) = self.tabs.get(self.active_tab)
            && let Some(terminal) = tab.active_terminal()
        {
            // Write the response as input to the terminal
            terminal.write_input(cleaned.as_bytes());
        }

        cx.notify();
    }

    pub(super) fn render_ai_input_modal(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let style = CommandPaletteStyle::resolve(self);
        let input_font = Font {
            family: self.font_family.clone(),
            ..Font::default()
        };

        div()
            .id("ai-input-modal")
            .size_full()
            .absolute()
            .top_0()
            .left_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_ai_input(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .pt(px(36.0))
                    .child(
                        div()
                            .id("ai-input-panel")
                            .w(px(AI_INPUT_WIDTH))
                            .px(px(10.0))
                            .py(px(10.0))
                            .rounded(px(COMMAND_PALETTE_PANEL_RADIUS))
                            .bg(style.panel_bg)
                            .border_1()
                            .border_color(style.panel_border)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|_this, _event, _window, cx| {
                                    cx.stop_propagation();
                                }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .pb(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(style.muted_text)
                                    .child("AI Input"),
                            )
                            .child(
                                div()
                                    .id("ai-input-field")
                                    .w_full()
                                    .h(px(34.0))
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .relative()
                                    .rounded(px(COMMAND_PALETTE_INPUT_RADIUS))
                                    .bg(style.input_bg)
                                    .border_1()
                                    .border_color(style.panel_border)
                                    .child(div().w_full().h_full().relative().child(
                                        self.render_inline_input_layer(
                                            input_font.clone(),
                                            px(13.0),
                                            style.primary_text.into(),
                                            style.input_selection.into(),
                                            InlineInputAlignment::Left,
                                            cx,
                                        ),
                                    )),
                            )
                            .child(
                                div()
                                    .pt(px(8.0))
                                    .text_size(px(11.0))
                                    .text_color(style.muted_text)
                                    .child("Enter: Submit  Esc: Close"),
                            ),
                    ),
            )
            .into_any()
    }
}

fn extract_line_text_for_ai(
    grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
    line_idx: i32,
) -> Option<String> {
    let line = Line(line_idx);
    let cols = grid.columns();

    // Check if line is within grid bounds
    let total_lines = grid.total_lines();
    if line_idx < -(total_lines as i32 - grid.screen_lines() as i32)
        || line_idx >= grid.screen_lines() as i32
    {
        return None;
    }

    let mut text = String::with_capacity(cols);
    for col in 0..cols {
        let cell = &grid[line][Column(col)];
        let c = cell.c;
        if c == '\0' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) || c.is_control() {
            text.push(' ');
        } else {
            text.push(c);
        }
    }

    Some(text)
}

/// Strip markdown code block formatting from AI responses.
/// Handles formats like:
/// ```bash
/// command
/// ```
/// or just:
/// ```
/// command
/// ```
fn strip_markdown_code_block(text: &str) -> String {
    let trimmed = text.trim();

    // Check if it starts with ``` and ends with ```
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        // Find the end of the first line (which may contain language identifier)
        let first_newline = trimmed.find('\n').unwrap_or(3);
        let last_backticks = trimmed.rfind("```").unwrap_or(trimmed.len());

        if first_newline < last_backticks {
            // Extract content between opening ``` line and closing ```
            let content = &trimmed[first_newline..last_backticks];
            return content.trim().to_string();
        }
    }

    // Also handle inline code with single backticks: `command`
    if trimmed.starts_with('`')
        && trimmed.ends_with('`')
        && !trimmed.starts_with("```")
        && trimmed.len() > 2
    {
        return trimmed[1..trimmed.len() - 1].to_string();
    }

    trimmed.to_string()
}
