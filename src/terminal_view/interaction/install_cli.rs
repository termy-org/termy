use super::*;

impl TerminalView {
    pub(in super::super) fn execute_install_cli_command_action(
        &mut self,
        action: CommandAction,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::InstallCli => {
                self.install_cli_action(cx);
                true
            }
            _ => false,
        }
    }

    pub(in super::super) fn install_cli_action(&mut self, cx: &mut Context<Self>) {
        if !self.install_cli_available() {
            termy_toast::info("CLI is already installed");
            cx.notify();
            return;
        }

        match termy_cli_install_core::install_cli(self.terminal_runtime.shell.as_deref()) {
            Ok(result) => {
                let install_path = result.install_path;
                let path_str = install_path.display().to_string();

                if let Some(shell_setup) = result.shell_setup {
                    self.write_terminal_input(format!("{}\n", shell_setup.session_command).as_bytes(), cx);
                    if shell_setup.profile_updated {
                        termy_toast::success(format!(
                            "CLI installed to {}. Updated {} and activated PATH in this shell.",
                            path_str,
                            shell_setup.profile_path.display()
                        ));
                    } else {
                        termy_toast::success(format!(
                            "CLI installed to {}. {} already configures Termy PATH; activated PATH in this shell.",
                            path_str,
                            shell_setup.profile_path.display()
                        ));
                    }
                } else {
                    #[cfg(target_os = "windows")]
                    {
                        if let Some(parent) = install_path.parent() {
                            termy_toast::success(format!(
                                "CLI installed to {}. Add {} to PATH: setx PATH \"%PATH%;{}\"",
                                path_str,
                                parent.display(),
                                parent.display()
                            ));
                        } else {
                            termy_toast::success(format!("CLI installed to {}", path_str));
                        }
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        termy_toast::success(format!("CLI installed to {}", path_str));
                    }
                }

                if self.refresh_install_cli_availability() {
                    self.refresh_command_palette_items_for_current_mode(cx);
                }
                cx.notify();
            }
            Err(error) => {
                termy_toast::error(error);
                cx.notify();
            }
        }
    }
}
