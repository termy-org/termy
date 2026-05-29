fn main() {
    println!("cargo::rustc-check-cfg=cfg(macos_sdk_26)");

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
