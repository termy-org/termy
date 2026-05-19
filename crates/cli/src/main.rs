use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "termy")]
#[command(about = "Termy terminal emulator CLI", long_about = None)]
#[command(version)]
struct Cli {
    /// Open Termy with this working directory
    #[arg(
        long = "working-directory",
        value_name = "PATH",
        conflicts_with = "path"
    )]
    working_directory: Option<PathBuf>,

    /// Open Termy with this working directory
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

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

    /// Export the current resolved theme into a Termy themes repo checkout
    #[command(name = "-export-theme")]
    ExportTheme {
        /// Local path to the termy-org/themes checkout
        #[arg(long)]
        repo: PathBuf,
        /// Theme slug, normalized to Termy's theme id format
        #[arg(long)]
        slug: String,
        /// Display name for the theme
        #[arg(long)]
        name: String,
        /// Semver version, for example 1.0.0
        #[arg(long)]
        version: String,
        /// Theme description
        #[arg(long, default_value = "")]
        description: String,
        /// Overwrite an existing files/<version>.json
        #[arg(long)]
        force: bool,
    },

    /// Validate a Termy themes repo checkout
    #[command(name = "-validate-theme-repo")]
    ValidateThemeRepo {
        /// Local path to the termy-org/themes checkout
        #[arg(long)]
        repo: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Some(path) = cli.working_directory.or(cli.path) {
        commands::open::run(path);
        return;
    }

    match cli.action {
        Some(Action::Version) => commands::version::run(),
        Some(Action::Help) => commands::help::run(),
        Some(Action::ListFonts) => commands::list_fonts::run(),
        Some(Action::ListKeybinds) => commands::list_keybinds::run(),
        Some(Action::ListThemes) => commands::list_themes::run(),
        Some(Action::ListColors) => commands::list_colors::run(),
        Some(Action::ListActions) => commands::list_actions::run(),
        Some(Action::EditConfig) => commands::edit_config::run(),
        Some(Action::ShowConfig) => commands::show_config::run(),
        Some(Action::ValidateConfig) => commands::validate_config::run(),
        Some(Action::PrettifyConfig) => commands::prettify_config::run(),
        Some(Action::Tui) => commands::tui::run(),
        Some(Action::Update) => commands::update::run(),
        Some(Action::ExportTheme {
            repo,
            slug,
            name,
            version,
            description,
            force,
        }) => commands::theme_repo::export_theme(repo, slug, name, version, description, force),
        Some(Action::ValidateThemeRepo { repo }) => commands::theme_repo::validate_theme_repo(repo),
        None => {
            // No subcommand: show help
            commands::help::run();
        }
    }
}
