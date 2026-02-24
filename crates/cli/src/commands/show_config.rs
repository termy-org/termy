use crate::config::config_path;

pub fn run() {
    let path = match config_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config directory");
            return;
        }
    };

    if !path.exists() {
        println!("# Config file: {} (not created yet)", path.display());
        println!("# Using default configuration");
        println!();
        print_defaults();
        return;
    }

    println!("# Config file: {}", path.display());
    println!();

    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                println!("# (empty file - using defaults)");
                println!();
                print_defaults();
            } else {
                print!("{}", contents);
                if !contents.ends_with('\n') {
                    println!();
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to read config file: {}", e);
        }
    }
}

fn print_defaults() {
    println!("# Default values:");
    println!("theme = termy");
    println!("font_family = JetBrains Mono");
    println!("font_size = 14");
    println!("term = xterm-256color");
    println!("cursor_style = line");
    println!("cursor_blink = true");
    println!("background_opacity = 1.0");
    println!("padding_x = 12");
    println!("padding_y = 8");
    println!("scrollback_history = 10000");
}
