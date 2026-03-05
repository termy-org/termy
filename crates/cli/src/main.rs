use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "termy")]
#[command(about = "Termy terminal emulator CLI", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    action: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Show version information
    #[command(name = "-version")]
    Version,

    /// Show help and available actions
    #[command(name = "-help")]
    Help,

    /// List available monospace fonts
    #[command(name = "-list-fonts")]
    ListFonts,

    /// List all keybindings
    #[command(name = "-list-keybinds")]
    ListKeybinds,

    /// List discovered plugins
    #[command(name = "-list-plugins")]
    ListPlugins,

    /// List available themes
    #[command(name = "-list-themes")]
    ListThemes,

    /// Show current theme colors
    #[command(name = "-list-colors")]
    ListColors,

    /// List available keybind actions
    #[command(name = "-list-actions")]
    ListActions,

    /// Open config file in editor
    #[command(name = "-edit-config")]
    EditConfig,

    /// Display current configuration
    #[command(name = "-show-config")]
    ShowConfig,

    /// Validate configuration file
    #[command(name = "-validate-config")]
    ValidateConfig,

    /// Prettify configuration file (removes comments, formats consistently)
    #[command(name = "-prettify-config")]
    PrettifyConfig,

    /// Interactive TUI for all CLI features
    #[command(name = "-tui")]
    Tui,

    /// Check for updates
    #[command(name = "-update")]
    Update,
}

fn main() {
    let cli = Cli::parse();

    match cli.action {
        Some(Action::Version) => commands::version::run(),
        Some(Action::Help) => commands::help::run(),
        Some(Action::ListFonts) => commands::list_fonts::run(),
        Some(Action::ListKeybinds) => commands::list_keybinds::run(),
        Some(Action::ListPlugins) => commands::list_plugins::run(),
        Some(Action::ListThemes) => commands::list_themes::run(),
        Some(Action::ListColors) => commands::list_colors::run(),
        Some(Action::ListActions) => commands::list_actions::run(),
        Some(Action::EditConfig) => commands::edit_config::run(),
        Some(Action::ShowConfig) => commands::show_config::run(),
        Some(Action::ValidateConfig) => commands::validate_config::run(),
        Some(Action::PrettifyConfig) => commands::prettify_config::run(),
        Some(Action::Tui) => commands::tui::run(),
        Some(Action::Update) => commands::update::run(),
        None => {
            // No subcommand: show help
            commands::help::run();
        }
    }
}
