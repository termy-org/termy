use anyhow::{Context, Result};
use gpui::{App, AsyncApp, WeakEntity};
use std::path::PathBuf;
pub use termy_release_core::ReleaseInfo;
use termy_release_core::{UpdateCheck, check_for_updates};

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        url: String,
        extension: String,
    },
    Downloading {
        version: String,
        downloaded: u64,
        total: u64,
    },
    Downloaded {
        version: String,
        installer_path: PathBuf,
    },
    Installing {
        version: String,
    },
    Installed {
        version: String,
    },
    Error(String),
}

pub struct AutoUpdater {
    current_version: &'static str,
    pub state: UpdateState,
}

impl AutoUpdater {
    pub fn new(current_version: &'static str) -> Self {
        Self {
            current_version,
            state: UpdateState::Idle,
        }
    }

    pub fn check(entity: WeakEntity<Self>, cx: &mut App) {
        let Some(this) = entity.upgrade() else { return };
        this.update(cx, |this, cx| {
            this.state = UpdateState::Checking;
            cx.notify();
        });

        let current_version = this.read(cx).current_version.to_string();
        let bg = cx
            .background_executor()
            .spawn(async move { check_for_updates(&current_version) });

        let weak = entity.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let result = bg.await;
            cx.update(|cx| {
                let Some(this) = weak.upgrade() else { return };
                this.update(cx, |this, cx| {
                    match result {
                        Ok(UpdateCheck::UpdateAvailable(info)) => {
                            this.state = UpdateState::Available {
                                version: info.version,
                                url: info.download_url,
                                extension: info.extension,
                            };
                        }
                        Ok(UpdateCheck::UpToDate) => {
                            this.state = UpdateState::UpToDate;
                        }
                        Err(e) => {
                            log::warn!("Update check failed: {}", e);
                            this.state = UpdateState::Error(format!("{}", e));
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub fn install(entity: WeakEntity<Self>, cx: &mut App) {
        let Some(this) = entity.upgrade() else { return };

        let (version, url, extension) = {
            let read = this.read(cx);
            match &read.state {
                UpdateState::Available {
                    version,
                    url,
                    extension,
                } => (version.clone(), url.clone(), extension.clone()),
                _ => return,
            }
        };

        this.update(cx, |this, cx| {
            this.state = UpdateState::Downloading {
                version: version.clone(),
                downloaded: 0,
                total: 0,
            };
            cx.notify();
        });

        let (progress_tx, progress_rx) = flume::bounded::<(u64, u64)>(4);
        let dest = cache_installer_path(&version, &extension);
        let dl_version = version.clone();
        let bg = cx
            .background_executor()
            .spawn(async move { download_installer(&url, &dest, progress_tx) });

        // Progress reader
        let weak_progress = entity.clone();
        let progress_version = version.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            while let Ok((downloaded, total)) = progress_rx.recv_async().await {
                let Some(this) = weak_progress.upgrade() else {
                    break;
                };
                let ver = progress_version.clone();
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.state = UpdateState::Downloading {
                            version: ver,
                            downloaded,
                            total,
                        };
                        cx.notify();
                    });
                });
            }
        })
        .detach();

        // Download completion
        let weak_done = entity.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let result = bg.await;
            cx.update(|cx| {
                let Some(this) = weak_done.upgrade() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    match result {
                        Ok(path) => {
                            this.state = UpdateState::Downloaded {
                                version: dl_version,
                                installer_path: path,
                            };
                        }
                        Err(e) => {
                            this.state = UpdateState::Error(format!("Download failed: {}", e));
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub fn complete_install(entity: WeakEntity<Self>, cx: &mut App) {
        let Some(this) = entity.upgrade() else { return };

        let (version, installer_path) = {
            let read = this.read(cx);
            match &read.state {
                UpdateState::Downloaded {
                    version,
                    installer_path,
                } => (version.clone(), installer_path.clone()),
                _ => return,
            }
        };

        this.update(cx, |this, cx| {
            this.state = UpdateState::Installing {
                version: version.clone(),
            };
            cx.notify();
        });

        let bg = cx
            .background_executor()
            .spawn(async move { do_install(&installer_path) });

        let weak = entity.clone();
        cx.spawn(async move |cx: &mut AsyncApp| {
            let result = bg.await;
            cx.update(|cx| {
                let Some(this) = weak.upgrade() else { return };
                this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            this.state = UpdateState::Installed { version };
                        }
                        Err(e) => {
                            this.state = UpdateState::Error(format!("Install failed: {}", e));
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub fn dismiss(&mut self, cx: &mut gpui::Context<Self>) {
        self.state = UpdateState::Idle;
        cx.notify();
    }
}

#[cfg(target_os = "macos")]
fn cache_installer_path(version: &str, extension: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache_dir = PathBuf::from(home).join("Library/Caches/Termy");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join(format!("update-{}.{}", version, extension))
}

#[cfg(target_os = "windows")]
fn cache_installer_path(version: &str, extension: &str) -> PathBuf {
    let cache_dir = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("Termy")
        .join("Cache");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join(format!("update-{}.{}", version, extension))
}

#[cfg(target_os = "linux")]
fn cache_installer_path(version: &str, extension: &str) -> PathBuf {
    // Use XDG_CACHE_HOME or ~/.cache
    let cache_dir = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".cache")
        })
        .join("termy");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join(format!("update-{}.{}", version, extension))
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn cache_installer_path(version: &str, extension: &str) -> PathBuf {
    let cache_dir = std::env::temp_dir().join("termy-updates");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join(format!("update-{}.{}", version, extension))
}

fn download_installer(
    url: &str,
    dest: &PathBuf,
    progress_tx: flume::Sender<(u64, u64)>,
) -> Result<PathBuf> {
    let response = ureq::get(url)
        .set("User-Agent", "Termy-Updater/1.0")
        .call()
        .context("Failed to download installer")?;

    let total: u64 = response
        .header("Content-Length")
        .and_then(|h| h.parse().ok())
        .unwrap_or(0);

    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(dest).context("Failed to create installer file")?;
    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 65536]; // 64KiB chunks

    loop {
        let n = reader
            .read(&mut buf)
            .context("Failed to read download stream")?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        downloaded += n as u64;
        let _ = progress_tx.try_send((downloaded, total));
    }

    Ok(dest.clone())
}

#[cfg(target_os = "macos")]
fn do_install(dmg_path: &PathBuf) -> Result<()> {
    use std::process::Command;

    // Mount the DMG
    let mount = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly"])
        .arg(dmg_path)
        .output()
        .context("Failed to mount DMG")?;

    if !mount.status.success() {
        anyhow::bail!(
            "hdiutil attach failed: {}",
            String::from_utf8_lossy(&mount.stderr)
        );
    }

    let mount_stdout = String::from_utf8_lossy(&mount.stdout);
    let mount_point = mount_stdout
        .lines()
        .find_map(|line| {
            line.find("/Volumes/")
                .map(|start| PathBuf::from(line[start..].trim()))
        })
        .context(format!(
            "Could not determine mounted volume from hdiutil output: {}",
            mount_stdout.trim()
        ))?;

    let install_result: Result<()> = (|| {
        let mut app_path = None;
        for entry in std::fs::read_dir(&mount_point).context("Failed to read mounted volume")? {
            let entry = entry?;
            let path = entry.path();
            let is_app = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("app"))
                .unwrap_or(false);
            if !is_app {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("Termy.app") {
                app_path = Some(path);
                break;
            }
            if app_path.is_none() {
                app_path = Some(path);
            }
        }

        let app_path = app_path.context("No .app bundle found inside mounted DMG")?;
        let target_app = PathBuf::from("/Applications").join(
            app_path
                .file_name()
                .context("Mounted app bundle is missing file name")?,
        );

        if target_app.exists() {
            let rm_result = Command::new("rm")
                .arg("-rf")
                .arg(&target_app)
                .output()
                .context("Failed to remove old app bundle in /Applications")?;
            if !rm_result.status.success() {
                anyhow::bail!(
                    "failed removing existing app: {}",
                    String::from_utf8_lossy(&rm_result.stderr)
                );
            }
        }

        // Use ditto for macOS app bundles to preserve metadata and avoid nested .app copies.
        let copy_result = Command::new("ditto")
            .arg(&app_path)
            .arg(&target_app)
            .output()
            .context("Failed to copy app bundle to /Applications")?;

        if !copy_result.status.success() {
            anyhow::bail!(
                "ditto failed: {}",
                String::from_utf8_lossy(&copy_result.stderr)
            );
        }

        Ok(())
    })();

    // Always try to detach, even if install failed.
    let _ = Command::new("hdiutil")
        .arg("detach")
        .arg(&mount_point)
        .arg("-quiet")
        .output();

    install_result
}

#[cfg(target_os = "windows")]
fn do_install(installer_path: &PathBuf) -> Result<()> {
    use std::process::Command;

    let extension = installer_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match extension {
        "msi" => {
            // Run MSI installer silently
            let result = Command::new("msiexec")
                .args([
                    "/i",
                    &installer_path.to_string_lossy(),
                    "/passive",
                    "/norestart",
                ])
                .output()
                .context("Failed to run MSI installer")?;

            if !result.status.success() {
                anyhow::bail!(
                    "MSI installation failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
        "exe" => {
            // Run Inno Setup installer with silent flags
            // /VERYSILENT: No UI at all
            // /SUPPRESSMSGBOXES: Suppress message boxes
            // /NORESTART: Don't restart after install
            // /CLOSEAPPLICATIONS: Close running instances of the app
            let result = Command::new(installer_path)
                .args([
                    "/VERYSILENT",
                    "/SUPPRESSMSGBOXES",
                    "/NORESTART",
                    "/CLOSEAPPLICATIONS",
                ])
                .output()
                .context("Failed to run EXE installer")?;

            if !result.status.success() {
                anyhow::bail!(
                    "Installer failed with exit code: {:?}",
                    result.status.code()
                );
            }
        }
        _ => {
            anyhow::bail!("Unsupported installer format: {}", extension);
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn do_install(tarball_path: &PathBuf) -> Result<()> {
    use std::process::Command;

    // Determine install directory: prefer ~/.local/bin, fall back to ~/bin
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let home_path = PathBuf::from(&home);

    let install_dir = if home_path.join(".local/bin").exists() {
        home_path.join(".local/bin")
    } else {
        let local_bin = home_path.join(".local/bin");
        std::fs::create_dir_all(&local_bin).context("Failed to create ~/.local/bin")?;
        local_bin
    };

    // Extract tarball to temp directory
    let temp_dir = std::env::temp_dir().join("termy-update-extract");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).context("Failed to create temp extraction directory")?;

    let tar_result = Command::new("tar")
        .args([
            "-xzf",
            &tarball_path.to_string_lossy(),
            "-C",
            &temp_dir.to_string_lossy(),
        ])
        .output()
        .context("Failed to extract tarball")?;

    if !tar_result.status.success() {
        anyhow::bail!(
            "tar extraction failed: {}",
            String::from_utf8_lossy(&tar_result.stderr)
        );
    }

    // Find the termy binary in the extracted contents
    let binary_path = temp_dir.join("termy/termy");
    let alt_binary_path = temp_dir.join("termy");

    let source_binary = if binary_path.exists() {
        binary_path
    } else if alt_binary_path.is_file() {
        alt_binary_path
    } else {
        // Search for the binary
        let mut found = None;
        for entry in std::fs::read_dir(&temp_dir).context("Failed to read temp directory")? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let potential = path.join("termy");
                if potential.exists() {
                    found = Some(potential);
                    break;
                }
            }
        }
        found.context("Could not find termy binary in extracted tarball")?
    };

    // Copy binary to install directory
    let target_binary = install_dir.join("termy");
    std::fs::copy(&source_binary, &target_binary)
        .context("Failed to copy binary to install directory")?;

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target_binary)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target_binary, perms)?;
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn do_install(_installer_path: &PathBuf) -> Result<()> {
    anyhow::bail!("Auto-install is only supported on macOS, Windows, and Linux")
}
