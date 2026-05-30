fn main() {
    println!("cargo::rustc-check-cfg=cfg(macos_sdk_26)");

    #[cfg(target_os = "macos")]
    {
        use std::path::{Path, PathBuf};
        use std::process::Command;

        fn swift_toolchain_lib_dir() -> Option<PathBuf> {
            let output = Command::new("xcrun")
                .args(["--find", "swiftc"])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }

            let swiftc = String::from_utf8(output.stdout).ok()?;
            let swiftc = PathBuf::from(swiftc.trim());
            let bin_dir = swiftc.parent()?;
            let usr_dir = bin_dir.parent()?;
            Some(usr_dir.join("lib"))
        }

        fn swift_macos_rpaths(toolchain_lib_dir: &Path) -> Vec<PathBuf> {
            let mut rpaths = vec![PathBuf::from("/usr/lib/swift")];
            let bundled_runtime = toolchain_lib_dir.join("swift/macosx");
            if bundled_runtime.is_dir() {
                rpaths.push(bundled_runtime);
            }

            if let Ok(entries) = std::fs::read_dir(toolchain_lib_dir) {
                let mut compatibility_paths = entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.is_dir()
                            && path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| name.starts_with("swift-"))
                    })
                    .map(|path| path.join("macosx"))
                    .filter(|path| path.is_dir())
                    .collect::<Vec<_>>();
                compatibility_paths.sort();
                rpaths.extend(compatibility_paths);
            }

            rpaths
        }

        println!("cargo::rerun-if-env-changed=DEVELOPER_DIR");
        if let Some(toolchain_lib_dir) = swift_toolchain_lib_dir() {
            for rpath in swift_macos_rpaths(&toolchain_lib_dir) {
                println!("cargo::rustc-link-arg=-Wl,-rpath,{}", rpath.display());
            }
        }

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
        let icon_path = "../../assets/termy.ico";
        if std::path::Path::new(icon_path).exists() {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(icon_path);
            if let Err(err) = res.compile() {
                panic!("failed to compile Windows resources: {err}");
            }
        }
    }
}
