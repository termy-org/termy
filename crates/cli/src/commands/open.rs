use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn run(path: PathBuf) {
    match launch_termy(path) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn launch_termy(path: PathBuf) -> Result<(), String> {
    let working_dir = resolve_working_dir(&path)?;
    let app_binary = find_termy_app_binary()?;

    Command::new(&app_binary)
        .arg("--working-directory")
        .arg(&working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to launch {}: {error}", app_binary.display()))?;

    Ok(())
}

fn resolve_working_dir(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Failed to resolve current directory: {error}"))?
            .join(path)
    };

    let path = path
        .canonicalize()
        .map_err(|error| format!("Failed to resolve {}: {error}", path.display()))?;

    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }

    Ok(path)
}

fn find_termy_app_binary() -> Result<PathBuf, String> {
    let exe_path =
        std::env::current_exe().map_err(|error| format!("Failed to resolve CLI path: {error}"))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| format!("CLI path {} has no parent directory", exe_path.display()))?;

    let app_binary_name = format!("termy{}", std::env::consts::EXE_SUFFIX);
    let sibling = exe_dir.join(&app_binary_name);
    if is_executable_file(&sibling) && sibling != exe_path {
        return Ok(sibling);
    }

    for candidate in fallback_termy_app_binary_paths(&app_binary_name) {
        if is_executable_file(&candidate) && candidate != exe_path {
            return Ok(candidate);
        }
    }

    Err("Termy app binary not found. Build it with: cargo build -p termy".to_string())
}

fn fallback_termy_app_binary_paths(app_binary_name: &str) -> [PathBuf; 2] {
    [
        PathBuf::from("target/debug").join(app_binary_name),
        PathBuf::from("target/release").join(app_binary_name),
    ]
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use super::resolve_working_dir;

    #[test]
    fn open_resolves_existing_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let resolved = resolve_working_dir(temp.path()).expect("directory should resolve");
        assert_eq!(resolved, temp.path().canonicalize().unwrap());
    }

    #[test]
    fn open_rejects_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("file.txt");
        std::fs::write(&file, "content").expect("write file");
        let error = resolve_working_dir(&file).expect_err("file should be rejected");
        assert!(error.contains("is not a directory"));
    }
}
