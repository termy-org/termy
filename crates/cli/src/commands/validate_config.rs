use crate::config::config_path;

const VALID_KEYS: &[&str] = &[
    "theme",
    "font_family",
    "font_size",
    "term",
    "colorterm",
    "shell",
    "working_dir",
    "cursor_style",
    "cursor_blink",
    "background_opacity",
    "background_blur",
    "padding_x",
    "padding_y",
    "mouse_scroll_multiplier",
    "window_width",
    "window_height",
    "terminal_scrollbar_visibility",
    "terminal_scrollbar_style",
    "scrollback_history",
    "inactive_tab_scrollback",
    "warn_on_quit_with_running_process",
    "command_palette_show_keybinds",
    "keybind",
    "tab_title_mode",
    "tab_title_fallback",
    "tab_title_explicit_prefix",
    "tab_title_shell_integration",
    "tab_title_prompt_format",
    "tab_title_command_format",
    "tab_close_visibility",
    "tab_width_mode",
];

const VALID_SECTIONS: &[&str] = &["colors", "tab_title"];

const VALID_ACTIONS: &[&str] = &[
    "new_tab",
    "close_tab",
    "minimize_window",
    "rename_tab",
    "app_info",
    "native_sdk_example",
    "restart_app",
    "open_config",
    "open_settings",
    "import_colors",
    "switch_theme",
    "zoom_in",
    "zoom_out",
    "zoom_reset",
    "open_search",
    "check_for_updates",
    "quit",
    "toggle_command_palette",
    "copy",
    "paste",
    "close_search",
    "search_next",
    "search_previous",
    "toggle_search_case_sensitive",
    "toggle_search_regex",
    "unbind",
    "clear",
];

const VALID_THEMES: &[&str] = &[
    "termy",
    "tokyo-night",
    "catppuccin-mocha",
    "dracula",
    "gruvbox-dark",
    "nord",
    "solarized-dark",
    "one-dark",
    "monokai",
    "material-dark",
    "palenight",
    "tomorrow-night",
    "oceanic-next",
];

pub fn run() {
    let path = match config_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config directory");
            std::process::exit(1);
        }
    };

    println!("Config file: {}", path.display());

    if !path.exists() {
        println!("Status: File does not exist (using defaults)");
        println!("Result: Valid");
        return;
    }

    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            println!("Status: Failed to read file");
            println!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut in_section: Option<&str> = None;

    for (line_num, line) in contents.lines().enumerate() {
        let line_num = line_num + 1;
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check for section headers
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section_name = &trimmed[1..trimmed.len() - 1];
            if VALID_SECTIONS.contains(&section_name) {
                in_section = Some(section_name);
            } else {
                warnings.push(format!(
                    "Line {}: Unknown section [{}]",
                    line_num, section_name
                ));
                in_section = None;
            }
            continue;
        }

        // Parse key = value
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Inside a section, allow any keys
            if in_section.is_some() {
                continue;
            }

            // Check if key is valid
            if !VALID_KEYS.contains(&key) {
                warnings.push(format!("Line {}: Unknown key '{}'", line_num, key));
                continue;
            }

            // Validate specific keys
            match key {
                "theme" => {
                    if !VALID_THEMES.contains(&value) {
                        warnings.push(format!(
                            "Line {}: Unknown theme '{}'. Valid themes: {}",
                            line_num,
                            value,
                            VALID_THEMES.join(", ")
                        ));
                    }
                }
                "keybind" => {
                    if value == "clear" {
                        continue;
                    }
                    if let Some((_, action)) = value.split_once('=') {
                        let action = action.trim();
                        if !VALID_ACTIONS.contains(&action) {
                            warnings.push(format!(
                                "Line {}: Unknown keybind action '{}'",
                                line_num, action
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "Line {}: Invalid keybind format. Expected 'keybind = <trigger>=<action>'",
                            line_num
                        ));
                    }
                }
                "font_size" => {
                    if value.parse::<f32>().is_err() {
                        errors.push(format!("Line {}: font_size must be a number", line_num));
                    }
                }
                "background_opacity" => {
                    if let Ok(v) = value.parse::<f32>() {
                        if !(0.0..=1.0).contains(&v) {
                            errors.push(format!(
                                "Line {}: background_opacity must be between 0.0 and 1.0",
                                line_num
                            ));
                        }
                    } else {
                        errors.push(format!(
                            "Line {}: background_opacity must be a number",
                            line_num
                        ));
                    }
                }
                "cursor_style" => {
                    if !["line", "block"].contains(&value.to_lowercase().as_str()) {
                        errors.push(format!(
                            "Line {}: cursor_style must be 'line' or 'block'",
                            line_num
                        ));
                    }
                }
                "cursor_blink"
                | "background_blur"
                | "warn_on_quit_with_running_process"
                | "command_palette_show_keybinds"
                | "tab_title_shell_integration" => {
                    if !["true", "false"].contains(&value.to_lowercase().as_str()) {
                        errors.push(format!(
                            "Line {}: {} must be 'true' or 'false'",
                            line_num, key
                        ));
                    }
                }
                "tab_close_visibility" => {
                    if ![
                        "active_hover",
                        "activehover",
                        "active+hover",
                        "hover",
                        "always",
                    ]
                    .contains(&value.to_lowercase().as_str())
                    {
                        errors.push(format!(
                            "Line {}: tab_close_visibility must be 'active_hover', 'hover', or 'always'",
                            line_num
                        ));
                    }
                }
                "tab_width_mode" => {
                    if ![
                        "stable",
                        "active_grow",
                        "activegrow",
                        "active-grow",
                        "active_grow_sticky",
                        "activegrowsticky",
                        "active-grow-sticky",
                    ]
                    .contains(&value.to_lowercase().as_str())
                    {
                        errors.push(format!(
                            "Line {}: tab_width_mode must be 'stable', 'active_grow', or 'active_grow_sticky'",
                            line_num
                        ));
                    }
                }
                "scrollback_history" | "inactive_tab_scrollback" => {
                    if value.parse::<usize>().is_err() {
                        errors.push(format!(
                            "Line {}: {} must be a positive integer",
                            line_num, key
                        ));
                    }
                }
                _ => {}
            }
        } else {
            errors.push(format!(
                "Line {}: Invalid syntax. Expected 'key = value'",
                line_num
            ));
        }
    }

    // Print results
    if errors.is_empty() && warnings.is_empty() {
        println!("Status: Valid");
    } else {
        if !errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &errors {
                println!("  {}", error);
            }
        }

        if !warnings.is_empty() {
            println!();
            println!("Warnings:");
            for warning in &warnings {
                println!("  {}", warning);
            }
        }

        println!();
        if errors.is_empty() {
            println!("Result: Valid (with warnings)");
        } else {
            println!("Result: Invalid");
            std::process::exit(1);
        }
    }
}
