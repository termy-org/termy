use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{self, stdout};

use crate::commands::list_keybinds::KeybindDirective;
use crate::commands::validate_config;
use crate::config::{config_path, parse_keybind_lines, parse_theme_id};

#[derive(Clone, Copy, PartialEq)]
enum MenuItem {
    ShowConfig,
    ListFonts,
    ListThemes,
    ListColors,
    ListKeybinds,
    ListActions,
    ValidateConfig,
    EditConfig,
}

impl MenuItem {
    fn all() -> Vec<Self> {
        vec![
            Self::ShowConfig,
            Self::ListFonts,
            Self::ListThemes,
            Self::ListColors,
            Self::ListKeybinds,
            Self::ListActions,
            Self::ValidateConfig,
            Self::EditConfig,
        ]
    }

    fn label(&self) -> &'static str {
        match self {
            Self::ShowConfig => "Show Config",
            Self::ListFonts => "List Fonts",
            Self::ListThemes => "List Themes",
            Self::ListColors => "List Colors",
            Self::ListKeybinds => "List Keybindings",
            Self::ListActions => "List Actions",
            Self::ValidateConfig => "Validate Config",
            Self::EditConfig => "Edit Config",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::ShowConfig => "Display current configuration settings",
            Self::ListFonts => "Show available monospace fonts on your system",
            Self::ListThemes => "List all built-in color themes",
            Self::ListColors => "Show colors for the current theme",
            Self::ListKeybinds => "Display all keyboard shortcuts",
            Self::ListActions => "List available keybind actions",
            Self::ValidateConfig => "Check configuration file for errors",
            Self::EditConfig => "Open config file in your editor",
        }
    }
}

struct App {
    menu_state: ListState,
    items: Vec<MenuItem>,
    content: Vec<String>,
    scroll_offset: u16,
    should_quit: bool,
    should_edit_config: bool,
}

impl App {
    fn new() -> Self {
        let mut state = ListState::default();
        state.select(Some(0));

        let items = MenuItem::all();
        let content = get_content_for_item(&items[0]);

        Self {
            menu_state: state,
            items,
            content,
            scroll_offset: 0,
            should_quit: false,
            should_edit_config: false,
        }
    }

    fn next(&mut self) {
        let i = match self.menu_state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.menu_state.select(Some(i));
        self.update_content();
    }

    fn previous(&mut self) {
        let i = match self.menu_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.menu_state.select(Some(i));
        self.update_content();
    }

    fn update_content(&mut self) {
        self.scroll_offset = 0;
        if let Some(i) = self.menu_state.selected() {
            self.content = get_content_for_item(&self.items[i]);
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    fn selected_item(&self) -> Option<&MenuItem> {
        self.menu_state.selected().map(|i| &self.items[i])
    }
}

fn get_content_for_item(item: &MenuItem) -> Vec<String> {
    match item {
        MenuItem::ShowConfig => get_show_config_content(),
        MenuItem::ListFonts => get_list_fonts_content(),
        MenuItem::ListThemes => get_list_themes_content(),
        MenuItem::ListColors => get_list_colors_content(),
        MenuItem::ListKeybinds => get_list_keybinds_content(),
        MenuItem::ListActions => get_list_actions_content(),
        MenuItem::ValidateConfig => get_validate_config_content(),
        MenuItem::EditConfig => vec!["Press Enter to open config in your editor".to_string()],
    }
}

fn get_show_config_content() -> Vec<String> {
    let mut lines = Vec::new();

    let path = match config_path() {
        Some(p) => p,
        None => {
            lines.push("Could not determine config directory".to_string());
            return lines;
        }
    };

    lines.push(format!("Config file: {}", path.display()));
    lines.push(String::new());

    if !path.exists() {
        lines.push("(not created yet - using defaults)".to_string());
        lines.push(String::new());
        lines.push("Default values:".to_string());
        lines.push("  theme = termy".to_string());
        lines.push("  font_family = JetBrains Mono".to_string());
        lines.push("  font_size = 14".to_string());
        lines.push("  term = xterm-256color".to_string());
        lines.push("  cursor_style = line".to_string());
        lines.push("  cursor_blink = true".to_string());
        lines.push("  background_opacity = 1.0".to_string());
        lines.push("  padding_x = 12".to_string());
        lines.push("  padding_y = 8".to_string());
        lines.push("  scrollback_history = 10000".to_string());
        return lines;
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                lines.push("(empty file - using defaults)".to_string());
            } else {
                for line in contents.lines() {
                    lines.push(line.to_string());
                }
            }
        }
        Err(e) => {
            lines.push(format!("Failed to read config file: {}", e));
        }
    }

    lines
}

#[cfg(target_os = "macos")]
fn get_list_fonts_content() -> Vec<String> {
    use core_text::font_collection::create_for_all_families;

    let collection = create_for_all_families();
    let descriptors = collection.get_descriptors();

    let mut fonts: Vec<String> = Vec::new();

    if let Some(descriptors) = descriptors {
        for i in 0..descriptors.len() {
            if let Some(descriptor) = descriptors.get(i) {
                let family_name = descriptor.family_name();
                if !fonts.contains(&family_name) {
                    fonts.push(family_name);
                }
            }
        }
    }

    fonts.sort();
    fonts
}

#[cfg(target_os = "linux")]
fn get_list_fonts_content() -> Vec<String> {
    use std::process::Command;

    let output = Command::new("fc-list")
        .args([":spacing=mono", "-f", "%{family}\n"])
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut fonts: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
                fonts.sort();
                fonts.dedup();
                fonts.into_iter().filter(|s| !s.is_empty()).collect()
            } else {
                get_common_monospace_fonts()
            }
        }
        Err(_) => get_common_monospace_fonts(),
    }
}

#[cfg(target_os = "linux")]
fn get_common_monospace_fonts() -> Vec<String> {
    vec![
        "DejaVu Sans Mono".to_string(),
        "Liberation Mono".to_string(),
        "Fira Code".to_string(),
        "JetBrains Mono".to_string(),
        "Source Code Pro".to_string(),
        "Hack".to_string(),
        "Inconsolata".to_string(),
        "Ubuntu Mono".to_string(),
        "Droid Sans Mono".to_string(),
        "Roboto Mono".to_string(),
        "Cascadia Code".to_string(),
        "IBM Plex Mono".to_string(),
    ]
}

#[cfg(target_os = "windows")]
fn get_list_fonts_content() -> Vec<String> {
    vec![
        "Consolas".to_string(),
        "Courier New".to_string(),
        "Lucida Console".to_string(),
        "Cascadia Code".to_string(),
        "Cascadia Mono".to_string(),
        "JetBrains Mono".to_string(),
        "Fira Code".to_string(),
        "Source Code Pro".to_string(),
        String::new(),
        "Note: This is a partial list of common monospace fonts.".to_string(),
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn get_list_fonts_content() -> Vec<String> {
    vec!["Font listing is not supported on this platform".to_string()]
}

fn get_list_themes_content() -> Vec<String> {
    vec![
        "termy".to_string(),
        "tokyo-night".to_string(),
        "catppuccin-mocha".to_string(),
        "dracula".to_string(),
        "gruvbox-dark".to_string(),
        "nord".to_string(),
        "solarized-dark".to_string(),
        "one-dark".to_string(),
        "monokai".to_string(),
        "material-dark".to_string(),
        "palenight".to_string(),
        "tomorrow-night".to_string(),
        "oceanic-next".to_string(),
    ]
}

fn get_list_colors_content() -> Vec<String> {
    let theme_id = if let Some(path) = config_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            parse_theme_id(&contents).unwrap_or_else(|| "termy".to_string())
        } else {
            "termy".to_string()
        }
    } else {
        "termy".to_string()
    };

    let mut lines = vec![format!("Theme: {}", theme_id), String::new()];

    // Simplified color display - just show the theme name
    // Full color definitions would require duplicating the theme data
    lines.push("Use 'termy -list-colors' command for full color values".to_string());

    lines
}

fn get_list_keybinds_content() -> Vec<String> {
    #[derive(Clone, Copy, PartialEq)]
    #[allow(dead_code)]
    enum Platform {
        All,
        MacOs,
        Linux,
    }

    struct DefaultKeybind {
        trigger: &'static str,
        action: &'static str,
        #[allow(dead_code)]
        platform: Platform,
    }

    let default_keybinds: &[DefaultKeybind] = &[
        DefaultKeybind {
            trigger: "secondary-q",
            action: "quit",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-,",
            action: "open_config",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-p",
            action: "toggle_command_palette",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-t",
            action: "new_tab",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-w",
            action: "close_tab",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-m",
            action: "minimize_window",
            platform: Platform::MacOs,
        },
        DefaultKeybind {
            trigger: "secondary-=",
            action: "zoom_in",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-+",
            action: "zoom_in",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary--",
            action: "zoom_out",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-0",
            action: "zoom_reset",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-f",
            action: "open_search",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-g",
            action: "search_next",
            platform: Platform::All,
        },
        DefaultKeybind {
            trigger: "secondary-shift-g",
            action: "search_previous",
            platform: Platform::All,
        },
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        DefaultKeybind {
            trigger: "secondary-c",
            action: "copy",
            platform: Platform::All,
        },
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        DefaultKeybind {
            trigger: "secondary-v",
            action: "paste",
            platform: Platform::All,
        },
        #[cfg(target_os = "linux")]
        DefaultKeybind {
            trigger: "ctrl-shift-c",
            action: "copy",
            platform: Platform::Linux,
        },
        #[cfg(target_os = "linux")]
        DefaultKeybind {
            trigger: "ctrl-shift-v",
            action: "paste",
            platform: Platform::Linux,
        },
    ];

    let mut keybinds: Vec<(String, String)> = Vec::new();

    for kb in default_keybinds {
        #[cfg(target_os = "macos")]
        let is_current_platform = kb.platform == Platform::All || kb.platform == Platform::MacOs;
        #[cfg(target_os = "linux")]
        let is_current_platform = kb.platform == Platform::All || kb.platform == Platform::Linux;
        #[cfg(target_os = "windows")]
        let is_current_platform = kb.platform == Platform::All;

        if is_current_platform {
            keybinds.push((kb.trigger.to_string(), kb.action.to_string()));
        }
    }

    // Apply user config overrides
    if let Some(path) = config_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let directives = parse_keybind_lines(&contents);
            for directive in directives {
                match directive {
                    KeybindDirective::Clear => keybinds.clear(),
                    KeybindDirective::Bind { trigger, action } => {
                        keybinds.retain(|(t, _)| t != &trigger);
                        keybinds.push((trigger, action));
                    }
                    KeybindDirective::Unbind { trigger } => {
                        keybinds.retain(|(t, _)| t != &trigger);
                    }
                }
            }
        }
    }

    keybinds
        .iter()
        .map(|(trigger, action)| format!("{} = {}", trigger, action))
        .collect()
}

fn get_list_actions_content() -> Vec<String> {
    vec![
        "new_tab".to_string(),
        "close_tab".to_string(),
        "minimize_window".to_string(),
        "rename_tab".to_string(),
        "app_info".to_string(),
        "native_sdk_example".to_string(),
        "restart_app".to_string(),
        "open_config".to_string(),
        "open_settings".to_string(),
        "import_colors".to_string(),
        "switch_theme".to_string(),
        "zoom_in".to_string(),
        "zoom_out".to_string(),
        "zoom_reset".to_string(),
        "open_search".to_string(),
        "check_for_updates".to_string(),
        "quit".to_string(),
        "toggle_command_palette".to_string(),
        "copy".to_string(),
        "paste".to_string(),
        "close_search".to_string(),
        "search_next".to_string(),
        "search_previous".to_string(),
        "toggle_search_case_sensitive".to_string(),
        "toggle_search_regex".to_string(),
        "install_cli".to_string(),
    ]
}

fn get_validate_config_content() -> Vec<String> {
    let mut lines = Vec::new();

    let path = match config_path() {
        Some(p) => p,
        None => {
            lines.push("Could not determine config directory".to_string());
            return lines;
        }
    };

    if !path.exists() {
        lines.push("Config file does not exist yet".to_string());
        lines.push("Using default configuration (valid)".to_string());
        return lines;
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let validate_config::ValidationReport { errors, warnings } =
                validate_config::validate_contents(&contents);

            if errors.is_empty() && warnings.is_empty() {
                lines.push("Configuration is valid!".to_string());
            } else {
                if !errors.is_empty() {
                    lines.push("Errors:".to_string());
                    for error in errors {
                        lines.push(format!("  {}", error));
                    }
                }
                if !warnings.is_empty() {
                    if !lines.is_empty() {
                        lines.push(String::new());
                    }
                    lines.push("Warnings:".to_string());
                    for warning in warnings {
                        lines.push(format!("  {}", warning));
                    }
                }
            }
        }
        Err(e) => {
            lines.push(format!("Failed to read config file: {}", e));
        }
    }

    lines
}

pub fn run() {
    if let Err(e) = run_tui() {
        eprintln!("Error: {}", e);
    }
}

fn run_tui() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        KeyCode::Up | KeyCode::Char('k') => app.previous(),
                        KeyCode::Down | KeyCode::Char('j') => app.next(),
                        KeyCode::PageUp => {
                            for _ in 0..5 {
                                app.scroll_up();
                            }
                        }
                        KeyCode::PageDown => {
                            for _ in 0..5 {
                                app.scroll_down();
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(MenuItem::EditConfig) = app.selected_item() {
                                app.should_edit_config = true;
                                app.should_quit = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    if app.should_edit_config {
        crate::commands::edit_config::run();
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(f.area());

    // Menu
    let items: Vec<ListItem> = app.items.iter().map(|i| ListItem::new(i.label())).collect();

    let menu = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Termy CLI ")
                .title_style(Style::default().bold()),
        )
        .highlight_style(Style::default().bg(Color::Rgb(60, 60, 80)).bold())
        .highlight_symbol("> ");

    f.render_stateful_widget(menu, chunks[0], &mut app.menu_state);

    // Content area
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(chunks[1]);

    // Description
    let description = if let Some(item) = app.selected_item() {
        item.description()
    } else {
        ""
    };

    let desc_widget = Paragraph::new(description)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Description "),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(desc_widget, content_chunks[0]);

    // Content
    let content_text = app.content.join("\n");
    let content_widget = Paragraph::new(content_text)
        .block(Block::default().borders(Borders::ALL).title(" Content "))
        .scroll((app.scroll_offset, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(content_widget, content_chunks[1]);

    // Help footer
    let help_text = " q/Esc: Quit | j/k or Up/Down: Navigate | PgUp/PgDn: Scroll ";
    let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));

    let help_area = Rect {
        x: 0,
        y: f.area().height.saturating_sub(1),
        width: f.area().width,
        height: 1,
    };
    f.render_widget(help, help_area);
}
