use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};

use tempfile::NamedTempFile;

use super::{ConfigIoError, DEFAULT_CONFIG};

static CONFIG_CHANGE_SUBSCRIBERS: LazyLock<Mutex<Vec<flume::Sender<()>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) fn notify_config_changed() {
    let Ok(mut subscribers) = CONFIG_CHANGE_SUBSCRIBERS.lock() else {
        return;
    };
    subscribers.retain(|tx| tx.send(()).is_ok());
}

pub fn subscribe_config_changes() -> flume::Receiver<()> {
    let (tx, rx) = flume::unbounded();
    if let Ok(mut subscribers) = CONFIG_CHANGE_SUBSCRIBERS.lock() {
        subscribers.push(tx);
    }
    rx
}

pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), ConfigIoError> {
    let parent = path
        .parent()
        .ok_or_else(|| ConfigIoError::InvalidConfigPath(path.to_path_buf()))?;
    let mut temp =
        NamedTempFile::new_in(parent).map_err(|source| ConfigIoError::CreateTempFile {
            path: path.to_path_buf(),
            source,
        })?;

    temp.write_all(contents.as_bytes())
        .map_err(|source| ConfigIoError::WriteConfig {
            path: path.to_path_buf(),
            source,
        })?;
    temp.flush().map_err(|source| ConfigIoError::WriteConfig {
        path: path.to_path_buf(),
        source,
    })?;
    temp.as_file()
        .sync_all()
        .map_err(|source| ConfigIoError::WriteConfig {
            path: path.to_path_buf(),
            source,
        })?;
    temp.persist(path)
        .map_err(|error| ConfigIoError::PersistTempFile {
            path: path.to_path_buf(),
            source: error.error,
        })?;

    Ok(())
}

pub fn ensure_config_file() -> Result<PathBuf, ConfigIoError> {
    let path = termy_config_core::config_path().ok_or(ConfigIoError::ConfigPathUnavailable)?;
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| ConfigIoError::InvalidConfigPath(path.clone()))?;
        fs::create_dir_all(parent).map_err(|source| ConfigIoError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
        write_atomic(&path, DEFAULT_CONFIG)?;
    }
    Ok(path)
}

pub fn open_config_file() -> Result<(), ConfigIoError> {
    let path = ensure_config_file()?;

    #[cfg(target_os = "macos")]
    {
        run_open_command("open", &[], &path)?;
    }

    #[cfg(target_os = "linux")]
    {
        run_open_command("xdg-open", &[], &path)?;
    }

    #[cfg(target_os = "windows")]
    {
        run_open_command("cmd", &["/C", "start", ""], &path)?;
    }

    Ok(())
}

fn run_open_command(
    command: &'static str,
    args: &[&str],
    path: &Path,
) -> Result<(), ConfigIoError> {
    let path = path.to_path_buf();
    let status = Command::new(command)
        .args(args)
        .arg(&path)
        .status()
        .map_err(|source| ConfigIoError::LaunchOpenCommand {
            command,
            path: path.clone(),
            source,
        })?;
    if !status.success() {
        return Err(ConfigIoError::OpenCommandFailed {
            command,
            path,
            status,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    #[test]
    fn write_atomic_replaces_file_without_extra_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.txt");

        write_atomic(&path, "theme = termy\n").expect("write initial");
        write_atomic(&path, "theme = nord\n").expect("write replacement");

        let contents = std::fs::read_to_string(&path).expect("read config");
        assert_eq!(contents, "theme = nord\n");

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(entries, vec!["config.txt".to_string()]);
    }
}
