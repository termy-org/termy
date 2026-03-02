use super::command_palette::style::{
    COMMAND_PALETTE_INPUT_RADIUS, COMMAND_PALETTE_PANEL_RADIUS, CommandPaletteStyle,
};
use super::inline_input::InlineInputAlignment;
use super::*;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use std::path::Path;

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

    fn current_agent_working_directory(&self) -> String {
        Self::predicted_prompt_cwd(
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        )
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.display().to_string())
        })
        .unwrap_or_else(|| ".".to_string())
    }

    pub(super) fn create_agent_session(&mut self, cx: &mut Context<Self>) {
        let cwd = self.current_agent_working_directory();
        let configured_provider = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent session create config load",
        )
        .config
        .ai_provider;
        let provider = termy_agent_sidebar::AgentProvider::from(configured_provider);
        let model = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent session model config load",
        )
        .config
        .openai_model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match configured_provider {
            config::AiProvider::OpenAi => termy_openai::DEFAULT_MODEL.to_string(),
            config::AiProvider::Gemini => termy_gemini::DEFAULT_MODEL.to_string(),
        });

        self.agent_sessions.new_session(cwd, provider, model);
        cx.notify();
    }

    pub(super) fn select_agent_session(&mut self, session_id: u64, cx: &mut Context<Self>) {
        self.agent_sessions.set_active_by_id(session_id);
        cx.notify();
    }

    fn ensure_active_agent_session(&mut self) -> u64 {
        let cwd = self.current_agent_working_directory();
        let configured_provider = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent session ensure config load",
        )
        .config
        .ai_provider;
        let provider = termy_agent_sidebar::AgentProvider::from(configured_provider);
        let model = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent session model config load",
        )
        .config
        .openai_model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match configured_provider {
            config::AiProvider::OpenAi => termy_openai::DEFAULT_MODEL.to_string(),
            config::AiProvider::Gemini => termy_gemini::DEFAULT_MODEL.to_string(),
        });

        self.agent_sessions.ensure_session(cwd, provider, model)
    }

    fn active_agent_session_provider(&self) -> config::AiProvider {
        self.agent_sessions
            .active_session()
            .map(|session| match session.provider {
                termy_agent_sidebar::AgentProvider::OpenAi => config::AiProvider::OpenAi,
                termy_agent_sidebar::AgentProvider::Gemini => config::AiProvider::Gemini,
            })
            .unwrap_or(config::AiProvider::OpenAi)
    }

    pub(super) fn ensure_agent_sidebar_ready(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_agent_session();
        self.refresh_agent_model_options(false, cx);
    }

    pub(super) fn set_active_agent_provider(
        &mut self,
        provider: config::AiProvider,
        cx: &mut Context<Self>,
    ) {
        self.ensure_active_agent_session();
        let model = match provider {
            config::AiProvider::OpenAi => termy_openai::DEFAULT_MODEL.to_string(),
            config::AiProvider::Gemini => termy_gemini::DEFAULT_MODEL.to_string(),
        };
        self.agent_sessions
            .set_active_provider(termy_agent_sidebar::AgentProvider::from(provider));
        self.agent_sessions.set_active_model(model);
        self.agent_provider_dropdown_open = false;
        self.agent_model_dropdown_open = false;
        self.agent_model_options.clear();
        self.agent_models_loaded_for_api_key = None;
        self.refresh_agent_model_options(true, cx);
        cx.notify();
    }

    pub(super) fn set_active_agent_model(&mut self, model: String, cx: &mut Context<Self>) {
        self.ensure_active_agent_session();
        self.agent_sessions.set_active_model(model);
        self.agent_model_dropdown_open = false;
        cx.notify();
    }

    pub(super) fn open_ai_input(&mut self, cx: &mut Context<Self>) {
        if self.ai_input_open {
            return;
        }

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

    pub(super) fn refresh_agent_model_options(&mut self, force: bool, cx: &mut Context<Self>) {
        self.ensure_active_agent_session();
        let provider = self.active_agent_session_provider();
        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent model options config load",
        );
        let api_key = match provider {
            config::AiProvider::OpenAi => loaded.config.openai_api_key,
            config::AiProvider::Gemini => loaded.config.gemini_api_key,
        }
        .filter(|value| !value.trim().is_empty());
        let Some(api_key) = api_key else {
            self.agent_model_options.clear();
            self.agent_models_loaded_for_api_key = None;
            self.agent_models_loading = false;
            return;
        };

        if self.agent_models_loading {
            return;
        }

        let already_loaded_for_key = self.agent_models_loaded_for_api_key.as_ref().is_some_and(
            |(loaded_provider, loaded_key)| *loaded_provider == provider && loaded_key == &api_key,
        );
        if !force && already_loaded_for_key {
            return;
        }

        self.agent_models_loading = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let request_provider = provider;
            let request_key = api_key.clone();
            let result = smol::unblock(move || match provider {
                config::AiProvider::OpenAi => termy_openai::OpenAiClient::new(api_key)
                    .fetch_chat_models()
                    .map(|models| models.into_iter().map(|model| model.id).collect::<Vec<_>>())
                    .map_err(|error| error.to_string()),
                config::AiProvider::Gemini => termy_gemini::GeminiClient::new(api_key)
                    .fetch_chat_models()
                    .map(|models| models.into_iter().map(|model| model.id).collect::<Vec<_>>())
                    .map_err(|error| error.to_string()),
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.agent_models_loading = false;
                    let active_provider = view.active_agent_session_provider();
                    if active_provider != request_provider {
                        return;
                    }

                    match result {
                        Ok(mut models) => {
                            models.sort_unstable();
                            models.dedup();
                            view.agent_model_options = models;
                            view.agent_models_loaded_for_api_key =
                                Some((request_provider, request_key));
                        }
                        Err(error) => {
                            view.agent_model_options.clear();
                            view.agent_models_loaded_for_api_key = None;
                            termy_toast::error(format!(
                                "Failed to fetch {} models: {}",
                                match request_provider {
                                    config::AiProvider::OpenAi => "OpenAI",
                                    config::AiProvider::Gemini => "Gemini",
                                },
                                error
                            ));
                        }
                    }

                    cx.notify();
                })
            });
        })
        .detach();
    }

    pub(super) fn handle_ai_input_key_down(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "escape" => {
                self.close_ai_input(cx);
            }
            "enter" => {
                let text = self.ai_input.text().trim().to_string();
                if !text.is_empty() {
                    self.submit_ai_message(text, true, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn submit_agent_sidebar_input(&mut self, cx: &mut Context<Self>) {
        let text = self.agent_sidebar_input.text().trim().to_string();
        if text.is_empty() {
            return;
        }

        let session_id = self.ensure_active_agent_session();
        let Some(session) = self.agent_sessions.active_session() else {
            termy_toast::error("No active agent session");
            return;
        };
        let provider = session.provider;
        let model = session.model.clone();
        let cwd = session.cwd.clone();
        self.agent_sessions
            .push_active_message(termy_agent_sidebar::AgentMessageRole::User, text.clone());
        self.agent_sessions.set_active_running(true);
        self.agent_sidebar_input.clear();
        cx.notify();
        self.schedule_agent_spinner_animation(cx);

        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "agent sidebar submit config load",
        );
        let provider_config: config::AiProvider = provider.into();
        let api_key = match provider_config {
            config::AiProvider::OpenAi => loaded.config.openai_api_key,
            config::AiProvider::Gemini => loaded.config.gemini_api_key,
        }
        .filter(|value| !value.trim().is_empty());
        let Some(api_key) = api_key else {
            self.agent_sessions.push_active_message(
                termy_agent_sidebar::AgentMessageRole::Error,
                format!(
                    "{} API key not configured",
                    match provider_config {
                        config::AiProvider::OpenAi => "OpenAI",
                        config::AiProvider::Gemini => "Gemini",
                    }
                ),
            );
            self.agent_sessions.set_active_running(false);
            cx.notify();
            return;
        };

        let terminal_context = self.get_terminal_context_for_ai();
        let history = self.agent_sessions.active_history_for_model();
        let (tx, rx) = flume::unbounded::<AgentEvent>();
        let this_weak = cx.entity().downgrade();
        cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
            while let Ok(event) = rx.recv_async().await {
                let _ = cx.update(|cx| {
                    this_weak.update(cx, |view, cx| {
                        if !view.agent_sessions.set_active_by_id(session_id) {
                            return;
                        }
                        match event {
                            AgentEvent::AssistantStart => {
                                view.agent_sessions.start_active_assistant_stream();
                            }
                            AgentEvent::AssistantReplace(content) => {
                                view.agent_sessions
                                    .replace_active_assistant_stream_content(content);
                            }
                            AgentEvent::AssistantFinish => {
                                view.agent_sessions.finish_active_assistant_stream();
                            }
                            AgentEvent::AssistantChunk(chunk) => {
                                view.agent_sessions.push_active_assistant_chunk(&chunk);
                            }
                            AgentEvent::ToolOutput(output) => {
                                view.agent_sessions.push_active_message(
                                    termy_agent_sidebar::AgentMessageRole::Tool,
                                    output,
                                );
                            }
                            AgentEvent::Error(error) => {
                                view.agent_sessions.push_active_message(
                                    termy_agent_sidebar::AgentMessageRole::Error,
                                    error,
                                );
                            }
                            AgentEvent::Done => {
                                view.agent_sessions.finish_active_assistant_stream();
                                view.agent_sessions.set_active_running(false);
                            }
                        }
                        cx.notify();
                    })
                });
            }
        })
        .detach();

        std::thread::spawn(move || {
            let system_prompt = "You are an AI terminal agent. You can answer directly or request a tool call.\n\
Use this JSON format for a tool call only when needed:\n\
{\"tool\":\"run_shell\",\"command\":\"<shell command>\"}\n\
Use only one command per tool call.\n\
Current terminal context is provided in the latest user message.";

            let mut openai_messages = vec![termy_openai::ChatMessage::system(system_prompt)];
            let mut gemini_messages = vec![termy_gemini::ChatMessage::system(system_prompt)];
            for (role, content) in history {
                match role.as_str() {
                    "assistant" => {
                        openai_messages.push(termy_openai::ChatMessage::assistant(content.clone()));
                        gemini_messages.push(termy_gemini::ChatMessage {
                            role: "assistant".to_string(),
                            content: termy_gemini::ChatContent::Text(content),
                        });
                    }
                    _ => {
                        openai_messages.push(termy_openai::ChatMessage::user(content.clone()));
                        gemini_messages.push(termy_gemini::ChatMessage::user(content));
                    }
                }
            }

            if let Some(last) = openai_messages.last_mut()
                && let termy_openai::ChatContent::Text(text) = &mut last.content
            {
                *text = format!(
                    "{}\n\nTerminal context:\n```\n{}\n```",
                    text, terminal_context
                );
            }
            if let Some(last) = gemini_messages.last_mut()
                && let termy_gemini::ChatContent::Text(text) = &mut last.content
            {
                *text = format!(
                    "{}\n\nTerminal context:\n```\n{}\n```",
                    text, terminal_context
                );
            }

            let result = match provider_config {
                config::AiProvider::OpenAi => {
                    let client = termy_openai::OpenAiClient::new(api_key).with_model(model.clone());
                    run_agent_turn_openai(&client, &cwd, openai_messages, &tx)
                }
                config::AiProvider::Gemini => {
                    let client = termy_gemini::GeminiClient::new(api_key).with_model(model.clone());
                    run_agent_turn_gemini(&client, &cwd, gemini_messages, &tx)
                }
            };

            if let Err(error) = result {
                let _ = tx.send(AgentEvent::Error(error));
            }
            let _ = tx.send(AgentEvent::Done);
        });
    }

    fn submit_ai_message(
        &mut self,
        user_message: String,
        close_ai_modal: bool,
        cx: &mut Context<Self>,
    ) {
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

        // Close or clear the input surface before sending
        if close_ai_modal {
            self.close_ai_input(cx);
        } else {
            self.agent_sidebar_input.clear();
            cx.notify();
        }
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
                        }
                    }
                    cx.notify();
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
        if let Some(tab) = self.tabs.get(self.active_tab) {
            if let Some(terminal) = tab.active_terminal() {
                // Write the response as input to the terminal
                terminal.write_input(cleaned.as_bytes());
            }
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

#[derive(Debug, Clone)]
enum AgentEvent {
    AssistantStart,
    AssistantReplace(String),
    AssistantFinish,
    AssistantChunk(String),
    ToolOutput(String),
    Error(String),
    Done,
}

fn run_agent_turn_openai(
    client: &termy_openai::OpenAiClient,
    cwd: &str,
    messages: Vec<termy_openai::ChatMessage>,
    tx: &flume::Sender<AgentEvent>,
) -> Result<(), String> {
    let _ = tx.send(AgentEvent::AssistantStart);
    let first = client
        .chat_stream(messages.clone(), |chunk| {
            let _ = tx.send(AgentEvent::AssistantChunk(chunk.to_string()));
        })
        .map_err(|error| error.to_string())?;

    if let Some(tool_call) = termy_agent_sidebar::parse_run_shell_tool_call(&first) {
        let _ = tx.send(AgentEvent::AssistantReplace(format!(
            "Using tool: {}",
            tool_call.command
        )));
        let _ = tx.send(AgentEvent::AssistantFinish);
        let tool_output =
            termy_agent_sidebar::execute_run_shell_tool(Path::new(cwd), &tool_call.command)?;
        let _ = tx.send(AgentEvent::ToolOutput(tool_output.clone()));

        let mut followup = messages;
        followup.push(termy_openai::ChatMessage::assistant(first));
        followup.push(termy_openai::ChatMessage::user(format!(
            "Tool output:\n{}\n\nRespond to the user with the result.",
            tool_output
        )));

        let _ = tx.send(AgentEvent::AssistantStart);
        client
            .chat_stream(followup, |chunk| {
                let _ = tx.send(AgentEvent::AssistantChunk(chunk.to_string()));
            })
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    Ok(())
}

fn run_agent_turn_gemini(
    client: &termy_gemini::GeminiClient,
    cwd: &str,
    messages: Vec<termy_gemini::ChatMessage>,
    tx: &flume::Sender<AgentEvent>,
) -> Result<(), String> {
    let _ = tx.send(AgentEvent::AssistantStart);
    let first = client
        .chat_stream(messages.clone(), |chunk| {
            let _ = tx.send(AgentEvent::AssistantChunk(chunk.to_string()));
        })
        .map_err(|error| error.to_string())?;

    if let Some(tool_call) = termy_agent_sidebar::parse_run_shell_tool_call(&first) {
        let _ = tx.send(AgentEvent::AssistantReplace(format!(
            "Using tool: {}",
            tool_call.command
        )));
        let _ = tx.send(AgentEvent::AssistantFinish);
        let tool_output =
            termy_agent_sidebar::execute_run_shell_tool(Path::new(cwd), &tool_call.command)?;
        let _ = tx.send(AgentEvent::ToolOutput(tool_output.clone()));

        let mut followup = messages;
        followup.push(termy_gemini::ChatMessage {
            role: "assistant".to_string(),
            content: termy_gemini::ChatContent::Text(first),
        });
        followup.push(termy_gemini::ChatMessage::user(format!(
            "Tool output:\n{}\n\nRespond to the user with the result.",
            tool_output
        )));

        let _ = tx.send(AgentEvent::AssistantStart);
        client
            .chat_stream(followup, |chunk| {
                let _ = tx.send(AgentEvent::AssistantChunk(chunk.to_string()));
            })
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    Ok(())
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
        if c == '\0' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            text.push(' ');
        } else if c.is_control() {
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
