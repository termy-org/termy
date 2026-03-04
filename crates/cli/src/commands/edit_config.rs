use std::process::Command;
use termy_config_core::config_path;

pub fn run() {
    let path = match config_path() {
        Some(p) => p,
        None => {
            eprintln!("Could not determine config directory");
            return;
        }
    };

    if !path.exists() {
        // Create parent directory if needed
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            eprintln!("Failed to create config directory: {}", e);
            return;
        }
        // Create empty config file
        if let Err(e) = std::fs::write(&path, "") {
            eprintln!("Failed to create config file: {}", e);
            return;
        }
    }

    println!("Opening {}", path.display());

    // Try $EDITOR first, then platform-specific fallbacks
    if let Ok(editor) = std::env::var("EDITOR") {
        let status = Command::new(&editor).arg(&path).status();

        match status {
            Ok(s) if s.success() => return,
            Ok(_) => eprintln!("Editor exited with error"),
            Err(e) => eprintln!("Failed to run {}: {}", editor, e),
        }
    }

    // Platform-specific fallbacks
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-t").arg(&path).status();
    }

    #[cfg(target_os = "linux")]
    {
        // Try xdg-open first, then common editors
        if Command::new("xdg-open").arg(&path).status().is_err() {
            for editor in &["nano", "vim", "vi"] {
                if Command::new(editor).arg(&path).status().is_ok() {
                    return;
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("notepad").arg(&path).status();
    }
}
