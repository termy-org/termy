#[cfg(not(target_os = "windows"))]
use std::{env, ffi::OsStr, path::PathBuf};

#[cfg(not(target_os = "windows"))]
const DEFAULT_SYSTEM_PATH_ENTRIES: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
#[cfg(not(target_os = "windows"))]
const EXTRA_PATH_ENTRIES: [&str; 4] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

#[cfg(not(target_os = "windows"))]
pub(crate) fn normalized_path_env(path: Option<&OsStr>) -> Option<String> {
    let mut path_entries: Vec<PathBuf> = path
        .map(env::split_paths)
        .map(|paths| paths.collect())
        .unwrap_or_default();
    path_entries.retain(|entry| !entry.as_os_str().is_empty());

    if path_entries.is_empty() {
        path_entries.extend(DEFAULT_SYSTEM_PATH_ENTRIES.into_iter().map(PathBuf::from));
    }

    for entry in EXTRA_PATH_ENTRIES {
        let entry = PathBuf::from(entry);
        if !path_entries.iter().any(|existing| existing == &entry) {
            path_entries.push(entry);
        }
    }

    env::join_paths(path_entries.iter())
        .ok()
        .map(|joined| joined.to_string_lossy().into_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::normalized_path_env;
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn normalized_path_env_starts_from_default_system_path_when_missing() {
        let path = normalized_path_env(None).expect("normalized path");
        let parsed = std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>();
        assert!(parsed.contains(&PathBuf::from("/usr/bin")));
        assert!(parsed.contains(&PathBuf::from("/bin")));
        assert!(parsed.contains(&PathBuf::from("/usr/sbin")));
        assert!(parsed.contains(&PathBuf::from("/sbin")));
        assert!(parsed.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(parsed.contains(&PathBuf::from("/opt/homebrew/sbin")));
        assert!(parsed.contains(&PathBuf::from("/usr/local/bin")));
        assert!(parsed.contains(&PathBuf::from("/usr/local/sbin")));
    }

    #[test]
    fn normalized_path_env_treats_empty_path_as_missing() {
        let raw = OsString::from("");
        let path = normalized_path_env(Some(raw.as_os_str())).expect("normalized path");
        let parsed = std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>();
        assert!(parsed.contains(&PathBuf::from("/usr/bin")));
        assert!(parsed.contains(&PathBuf::from("/bin")));
        assert!(parsed.contains(&PathBuf::from("/usr/sbin")));
        assert!(parsed.contains(&PathBuf::from("/sbin")));
    }

    #[test]
    fn normalized_path_env_appends_missing_entries_without_duplication() {
        let raw = OsString::from("/opt/homebrew/bin:/usr/bin:/bin");
        let path = normalized_path_env(Some(raw.as_os_str())).expect("normalized path");
        let parsed = std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>();
        let homebrew_bin = PathBuf::from("/opt/homebrew/bin");
        assert_eq!(
            parsed
                .iter()
                .filter(|entry| **entry == homebrew_bin)
                .count(),
            1
        );
        assert!(parsed.contains(&PathBuf::from("/opt/homebrew/sbin")));
        assert!(parsed.contains(&PathBuf::from("/usr/local/bin")));
        assert!(parsed.contains(&PathBuf::from("/usr/local/sbin")));
    }
}
