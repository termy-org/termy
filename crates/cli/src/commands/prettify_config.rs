use crate::config::config_path;
use std::collections::HashMap;

pub fn run() {
    let path = match config_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config directory");
            return;
        }
    };

    if !path.exists() {
        println!("Config file does not exist yet: {}", path.display());
        println!("Nothing to prettify.");
        return;
    }

    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read config file: {}", e);
            return;
        }
    };

    let prettified = prettify(&contents);

    match std::fs::write(&path, &prettified) {
        Ok(_) => {
            println!("Config file prettified: {}", path.display());
            println!();
            print!("{}", prettified);
        }
        Err(e) => {
            eprintln!("Failed to write config file: {}", e);
        }
    }
}

fn prettify(contents: &str) -> String {
    let mut settings: HashMap<String, String> = HashMap::new();
    let mut keybinds: Vec<String> = Vec::new();

    // Known setting keys in preferred order
    let setting_order = [
        "theme",
        "font_family",
        "font_size",
        "term",
        "cursor_style",
        "cursor_blink",
        "background_opacity",
        "padding_x",
        "padding_y",
        "scrollback_history",
    ];

    for line in contents.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            if key == "keybind" {
                keybinds.push(value.to_string());
            } else {
                settings.insert(key.to_string(), value.to_string());
            }
        }
    }

    let mut output = String::new();

    // Output settings in preferred order
    for key in &setting_order {
        if let Some(value) = settings.remove(*key) {
            output.push_str(&format!("{} = {}\n", key, value));
        }
    }

    // Output any remaining settings (unknown keys)
    let mut remaining: Vec<_> = settings.into_iter().collect();
    remaining.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in remaining {
        output.push_str(&format!("{} = {}\n", key, value));
    }

    // Output keybinds
    if !keybinds.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        for keybind in keybinds {
            output.push_str(&format!("keybind = {}\n", keybind));
        }
    }

    output
}
