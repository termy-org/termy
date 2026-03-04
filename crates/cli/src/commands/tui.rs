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

use crate::commands::{providers, validate_config};

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
        let index = match self.menu_state.selected() {
            Some(index) => {
                if index >= self.items.len() - 1 {
                    0
                } else {
                    index + 1
                }
            }
            None => 0,
        };
        self.menu_state.select(Some(index));
        self.update_content();
    }

    fn previous(&mut self) {
        let index = match self.menu_state.selected() {
            Some(index) => {
                if index == 0 {
                    self.items.len() - 1
                } else {
                    index - 1
                }
            }
            None => 0,
        };
        self.menu_state.select(Some(index));
        self.update_content();
    }

    fn update_content(&mut self) {
        self.scroll_offset = 0;
        if let Some(index) = self.menu_state.selected() {
            self.content = get_content_for_item(&self.items[index]);
        }
    }

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    fn selected_item(&self) -> Option<&MenuItem> {
        self.menu_state.selected().map(|index| &self.items[index])
    }
}

fn get_content_for_item(item: &MenuItem) -> Vec<String> {
    match item {
        MenuItem::ShowConfig => providers::show_config_lines().unwrap_or_else(|error| vec![error]),
        MenuItem::ListFonts => providers::list_fonts_lines(),
        MenuItem::ListThemes => providers::list_theme_lines(),
        MenuItem::ListColors => providers::list_color_lines(),
        MenuItem::ListKeybinds => format_provider_table_lines(providers::list_keybind_lines()),
        MenuItem::ListActions => format_provider_table_lines(providers::list_action_lines()),
        MenuItem::ValidateConfig => get_validate_config_content(),
        MenuItem::EditConfig => vec!["Press Enter to open config in your editor".to_string()],
    }
}

fn format_provider_table_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.replace('\t', "    "))
        .collect()
}

fn get_validate_config_content() -> Vec<String> {
    let path = match providers::config_file_path() {
        Ok(path) => path,
        Err(error) => return vec![error],
    };

    if !path.exists() {
        return vec![
            "Config file does not exist yet".to_string(),
            "Using default configuration (valid)".to_string(),
        ];
    }

    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) => return vec![format!("Failed to read config file: {}", error)],
    };

    let validate_config::ValidationReport { errors, warnings } =
        validate_config::validate_contents(&contents);

    if errors.is_empty() && warnings.is_empty() {
        return vec!["Configuration is valid!".to_string()];
    }

    let mut lines = Vec::new();

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

    lines
}

pub fn run() {
    if let Err(error) = run_tui() {
        eprintln!("Error: {}", error);
    }
}

fn run_tui() -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new();

    loop {
        terminal.draw(|frame| ui(frame, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
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

fn ui(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| ListItem::new(item.label()))
        .collect();

    let menu = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Termy CLI ")
                .title_style(Style::default().bold()),
        )
        .highlight_style(Style::default().bg(Color::Rgb(60, 60, 80)).bold())
        .highlight_symbol("> ");

    frame.render_stateful_widget(menu, chunks[0], &mut app.menu_state);

    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(chunks[1]);

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
    frame.render_widget(desc_widget, content_chunks[0]);

    let content_text = app.content.join("\n");
    let content_widget = Paragraph::new(content_text)
        .block(Block::default().borders(Borders::ALL).title(" Content "))
        .scroll((app.scroll_offset, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(content_widget, content_chunks[1]);

    let help_text = " q/Esc: Quit | j/k or Up/Down: Navigate | PgUp/PgDn: Scroll ";
    let help = Paragraph::new(help_text).style(Style::default().fg(Color::DarkGray));

    let help_area = Rect {
        x: 0,
        y: frame.area().height.saturating_sub(1),
        width: frame.area().width,
        height: 1,
    };
    frame.render_widget(help, help_area);
}
