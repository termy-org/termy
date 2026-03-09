fn main() {
    println!("cargo::rustc-check-cfg=cfg(macos_sdk_26)");
    generate_experimental_registry();

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        match Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-version"])
            .output()
        {
            Ok(output) if output.status.success() => match String::from_utf8(output.stdout) {
                Ok(sdk_version) => {
                    let major_version = sdk_version
                        .trim()
                        .split('.')
                        .next()
                        .and_then(|v| v.parse::<u32>().ok());

                    if let Some(major) = major_version
                        && major >= 26
                    {
                        println!("cargo::rustc-cfg=macos_sdk_26");
                    }
                }
                Err(err) => {
                    println!(
                        "cargo:warning=skipping macOS SDK cfg detection; non-UTF8 xcrun output: {err}"
                    );
                }
            },
            Ok(output) => {
                println!(
                    "cargo:warning=skipping macOS SDK cfg detection; xcrun exited with status {}",
                    output.status
                );
            }
            Err(err) => {
                println!(
                    "cargo:warning=skipping macOS SDK cfg detection; failed to execute xcrun: {err}"
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let icon_path = "assets/termy.ico";
        if std::path::Path::new(icon_path).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon_path);
            if let Err(err) = res.compile() {
                panic!("failed to compile Windows resources: {err}");
            }
        }
    }
}

fn generate_experimental_registry() {
    use std::fs;
    use std::path::Path;

    let crates_dir = Path::new("crates");
    println!("cargo:rerun-if-changed={}", crates_dir.display());

    let mut entries = fs::read_dir(crates_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("Cargo.toml"))
        .filter(|path| path.exists())
        .filter_map(|path| parse_experimental_manifest(&path))
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| left.title.cmp(&right.title));

    let generated = if entries.is_empty() {
        "&[]".to_string()
    } else {
        let body = entries
            .iter()
            .map(ExperimentalManifestEntry::to_rust)
            .collect::<Vec<_>>()
            .join(",\n");
        format!("&[\n{body}\n]")
    };

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR should be set by cargo");
    let output_path = Path::new(&out_dir).join("experimental_features.rs");
    fs::write(&output_path, generated).expect("failed to write generated experimental registry");
}

#[derive(Debug)]
struct ExperimentalManifestEntry {
    crate_name: String,
    title: String,
    summary: String,
    details: String,
    toggle_setting_key: Option<String>,
    settings_section: Option<String>,
}

impl ExperimentalManifestEntry {
    fn to_rust(&self) -> String {
        let settings_section = self
            .settings_section
            .as_deref()
            .map(|section| match section {
                "Appearance" => "Some(crate::settings_view::SettingsSection::Appearance)",
                "Terminal" => "Some(crate::settings_view::SettingsSection::Terminal)",
                "Tabs" => "Some(crate::settings_view::SettingsSection::Tabs)",
                "Experimental" => "Some(crate::settings_view::SettingsSection::Experimental)",
                "ThemeStore" => "Some(crate::settings_view::SettingsSection::ThemeStore)",
                "Plugins" => "Some(crate::settings_view::SettingsSection::Plugins)",
                "Advanced" => "Some(crate::settings_view::SettingsSection::Advanced)",
                "Colors" => "Some(crate::settings_view::SettingsSection::Colors)",
                "Keybindings" => "Some(crate::settings_view::SettingsSection::Keybindings)",
                other => panic!("unsupported experimental settings_section `{other}`"),
            })
            .unwrap_or("None");

        format!(
            "    ExperimentalFeature {{ crate_name: {:?}, title: {:?}, summary: {:?}, details: {:?}, toggle_setting_key: {:?}, settings_section: {} }}",
            self.crate_name,
            self.title,
            self.summary,
            self.details,
            self.toggle_setting_key,
            settings_section
        )
    }
}

fn parse_experimental_manifest(path: &std::path::Path) -> Option<ExperimentalManifestEntry> {
    let contents = std::fs::read_to_string(path).ok()?;
    let mut package_name = None;
    let mut title = None;
    let mut summary = None;
    let mut details = None;
    let mut toggle_setting_key = None;
    let mut settings_section = None;
    let mut section_path: Vec<String> = Vec::new();

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section_path = line[1..line.len() - 1]
                .split('.')
                .map(|part| part.trim().to_string())
                .collect();
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim();

        if section_matches(&section_path, &["package"]) && key == "name" {
            package_name = parse_toml_string(value);
            continue;
        }

        if !section_matches(
            &section_path,
            &["package", "metadata", "termy", "experimental"],
        ) {
            continue;
        }

        match key {
            "title" => title = parse_toml_string(value),
            "summary" => summary = parse_toml_string(value),
            "details" => details = parse_toml_string(value),
            "toggle_setting_key" => toggle_setting_key = parse_toml_string(value),
            "settings_section" => settings_section = parse_toml_string(value),
            _ => {}
        }
    }

    Some(ExperimentalManifestEntry {
        crate_name: package_name?,
        title: title?,
        summary: summary?,
        details: details?,
        toggle_setting_key,
        settings_section,
    })
}

fn parse_toml_string(value: &str) -> Option<String> {
    let trimmed = value.split('#').next()?.trim();
    if trimmed.len() < 2 || !trimmed.starts_with('"') || !trimmed.ends_with('"') {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].replace("\\\"", "\""))
}

fn section_matches(section_path: &[String], expected: &[&str]) -> bool {
    section_path
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
}
