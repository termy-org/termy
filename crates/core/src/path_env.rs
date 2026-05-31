use std::{env, ffi::OsStr, path::PathBuf};

#[cfg(target_os = "windows")]
use std::ffi::OsString;

#[cfg(not(target_os = "windows"))]
const DEFAULT_SYSTEM_PATH_ENTRIES: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
#[cfg(not(target_os = "windows"))]
const EXTRA_PATH_ENTRIES: [&str; 4] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
];

#[cfg(target_os = "windows")]
const WINDOWS_ENV_PATH_REGISTRY_KEYS: [(&str, &str); 2] = [
    (
        "HKLM",
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    ),
    ("HKCU", "Environment"),
];

fn push_unique_path(entries: &mut Vec<PathBuf>, entry: PathBuf) {
    if entry.as_os_str().is_empty() {
        return;
    }

    #[cfg(target_os = "windows")]
    let duplicate = {
        let candidate = entry.to_string_lossy().to_ascii_lowercase();
        entries
            .iter()
            .any(|existing| existing.to_string_lossy().to_ascii_lowercase() == candidate)
    };

    #[cfg(not(target_os = "windows"))]
    let duplicate = entries.iter().any(|existing| existing == &entry);

    if !duplicate {
        entries.push(entry);
    }
}

fn push_split_paths(entries: &mut Vec<PathBuf>, path: Option<&OsStr>) {
    if let Some(path) = path {
        for entry in env::split_paths(path) {
            push_unique_path(entries, entry);
        }
    }
}

#[cfg(target_os = "windows")]
fn expand_windows_env_vars(value: &OsStr) -> OsString {
    let mut input = value.to_string_lossy().into_owned();
    let mut output = String::with_capacity(input.len());

    while let Some(start) = input.find('%') {
        output.push_str(&input[..start]);
        let rest = &input[start + 1..];
        let Some(end) = rest.find('%') else {
            output.push('%');
            output.push_str(rest);
            input.clear();
            break;
        };

        let name = &rest[..end];
        if name.is_empty() {
            output.push_str("%%");
        } else if let Some(value) = env::var_os(name) {
            output.push_str(&value.to_string_lossy());
        } else {
            output.push('%');
            output.push_str(name);
            output.push('%');
        }
        input = rest[end + 1..].to_string();
    }

    output.push_str(&input);
    OsString::from(output)
}

#[cfg(target_os = "windows")]
fn registry_path_value(root: &str, subkey: &str) -> Option<OsString> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let root = match root {
        "HKLM" => RegKey::predef(HKEY_LOCAL_MACHINE),
        "HKCU" => RegKey::predef(HKEY_CURRENT_USER),
        _ => return None,
    };
    let key = root.open_subkey(subkey).ok()?;
    key.get_value::<OsString, _>("Path")
        .or_else(|_| key.get_value::<OsString, _>("PATH"))
        .ok()
}

#[cfg(target_os = "windows")]
fn registry_path_values() -> Vec<OsString> {
    WINDOWS_ENV_PATH_REGISTRY_KEYS
        .into_iter()
        .filter_map(|(root, subkey)| registry_path_value(root, subkey))
        .collect()
}

#[cfg(target_os = "windows")]
fn windows_system_path_entries() -> Vec<PathBuf> {
    let system_root = env::var_os("SystemRoot")
        .or_else(|| env::var_os("WINDIR"))
        .unwrap_or_else(|| OsString::from(r"C:\Windows"));
    let system_root = PathBuf::from(system_root);

    [
        system_root.join("System32"),
        system_root.clone(),
        system_root.join(r"System32\Wbem"),
        system_root.join(r"System32\WindowsPowerShell\v1.0"),
        system_root.join(r"System32\OpenSSH"),
    ]
    .into()
}

#[cfg(target_os = "windows")]
fn push_if_env_dir(entries: &mut Vec<PathBuf>, var: &str, relative: &str) {
    if let Some(base) = env::var_os(var).filter(|value| !value.is_empty()) {
        push_unique_path(entries, PathBuf::from(base).join(relative));
    }
}

#[cfg(target_os = "windows")]
fn push_if_env_path(entries: &mut Vec<PathBuf>, var: &str) {
    if let Some(path) = env::var_os(var).filter(|value| !value.is_empty()) {
        push_unique_path(entries, PathBuf::from(path));
    }
}

#[cfg(target_os = "windows")]
fn windows_tool_path_entries() -> Vec<PathBuf> {
    let mut entries = Vec::new();

    push_if_env_dir(&mut entries, "SCOOP", "shims");
    push_if_env_dir(&mut entries, "SCOOP_GLOBAL", "shims");
    push_if_env_dir(&mut entries, "USERPROFILE", r"scoop\shims");
    push_if_env_dir(&mut entries, "CARGO_HOME", "bin");
    push_if_env_dir(&mut entries, "USERPROFILE", r".cargo\bin");
    push_if_env_dir(&mut entries, "BUN_INSTALL", "bin");
    push_if_env_dir(&mut entries, "USERPROFILE", r".bun\bin");
    push_if_env_dir(&mut entries, "APPDATA", "npm");
    push_if_env_path(&mut entries, "PNPM_HOME");
    push_if_env_dir(&mut entries, "LOCALAPPDATA", "pnpm");
    push_if_env_dir(&mut entries, "LOCALAPPDATA", r"Microsoft\WindowsApps");

    entries
}

#[cfg(target_os = "windows")]
fn normalized_windows_path_env(
    path: Option<&OsStr>,
    registry_paths: impl IntoIterator<Item = OsString>,
    tool_paths: impl IntoIterator<Item = PathBuf>,
) -> Option<String> {
    let mut path_entries = Vec::new();
    push_split_paths(&mut path_entries, path);

    for entry in windows_system_path_entries() {
        push_unique_path(&mut path_entries, entry);
    }

    for value in registry_paths {
        let expanded = expand_windows_env_vars(&value);
        push_split_paths(&mut path_entries, Some(expanded.as_os_str()));
    }

    for entry in tool_paths {
        push_unique_path(&mut path_entries, entry);
    }

    env::join_paths(path_entries.iter())
        .ok()
        .map(|joined| joined.to_string_lossy().into_owned())
}

#[cfg(not(target_os = "windows"))]
pub fn normalized_path_env(path: Option<&OsStr>) -> Option<String> {
    let mut path_entries = Vec::new();
    push_split_paths(&mut path_entries, path);

    if path_entries.is_empty() {
        for entry in DEFAULT_SYSTEM_PATH_ENTRIES {
            push_unique_path(&mut path_entries, PathBuf::from(entry));
        }
    }

    for entry in EXTRA_PATH_ENTRIES {
        push_unique_path(&mut path_entries, PathBuf::from(entry));
    }

    env::join_paths(path_entries.iter())
        .ok()
        .map(|joined| joined.to_string_lossy().into_owned())
}

#[cfg(target_os = "windows")]
pub fn normalized_path_env(path: Option<&OsStr>) -> Option<String> {
    normalized_windows_path_env(path, registry_path_values(), windows_tool_path_entries())
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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::{expand_windows_env_vars, normalized_windows_path_env};
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn normalized_path_env_merges_registry_and_tool_paths() {
        let raw = OsString::from(r"C:\Windows\System32");
        let path = normalized_windows_path_env(
            Some(raw.as_os_str()),
            [OsString::from(r"C:\ProgramData\scoop\shims;C:\Tools\bin")],
            [
                PathBuf::from(r"C:\Users\me\scoop\shims"),
                PathBuf::from(r"C:\Users\me\.cargo\bin"),
                PathBuf::from(r"C:\Users\me\.bun\bin"),
            ],
        )
        .expect("normalized path");
        let parsed = std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>();

        assert!(parsed.contains(&PathBuf::from(r"C:\Windows\System32")));
        assert!(parsed.contains(&PathBuf::from(r"C:\ProgramData\scoop\shims")));
        assert!(parsed.contains(&PathBuf::from(r"C:\Tools\bin")));
        assert!(parsed.contains(&PathBuf::from(r"C:\Users\me\scoop\shims")));
        assert!(parsed.contains(&PathBuf::from(r"C:\Users\me\.cargo\bin")));
        assert!(parsed.contains(&PathBuf::from(r"C:\Users\me\.bun\bin")));
    }

    #[test]
    fn normalized_path_env_deduplicates_windows_paths_case_insensitively() {
        let raw = OsString::from(r"C:\Tools;c:\tools");
        let path = normalized_windows_path_env(
            Some(raw.as_os_str()),
            [OsString::from(r"C:\TOOLS")],
            [PathBuf::from(r"c:\TOOLS")],
        )
        .expect("normalized path");
        let parsed = std::env::split_paths(&OsString::from(path)).collect::<Vec<_>>();

        assert_eq!(
            parsed
                .iter()
                .filter(|entry| entry.to_string_lossy().eq_ignore_ascii_case(r"C:\Tools"))
                .count(),
            1
        );
    }

    #[test]
    fn expand_windows_env_vars_preserves_unknown_variables() {
        let expanded =
            expand_windows_env_vars(OsString::from(r"%TERMY_MISSING_VAR%\bin").as_os_str());
        assert_eq!(expanded, OsString::from(r"%TERMY_MISSING_VAR%\bin"));
    }
}
