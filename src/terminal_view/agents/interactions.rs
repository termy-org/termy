use super::*;

impl TerminalView {
    pub(in super::super) fn thread_project_id(&self, thread_id: &str) -> Option<&str> {
        self.agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.project_id.as_str())
    }

    pub(in super::super) fn begin_rename_agent_thread(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(initial_title) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| {
                thread
                    .custom_title
                    .clone()
                    .unwrap_or_else(|| self.agent_thread_display_title(thread))
            })
        else {
            return;
        };

        if self.is_command_palette_open() {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }
        if self.renaming_agent_project_id.is_some() {
            self.cancel_rename_agent_project(cx);
        }
        self.agent_sidebar_search_active = false;

        self.renaming_agent_thread_id = Some(thread_id.to_string());
        self.agent_thread_rename_input.set_text(initial_title);
        self.reset_cursor_blink_phase();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(in super::super) fn commit_rename_agent_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self.renaming_agent_thread_id.clone() else {
            return;
        };
        let Some(thread) = self
            .agent_threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            self.cancel_rename_agent_thread(cx);
            return;
        };

        let trimmed = self.agent_thread_rename_input.text().trim();
        thread.custom_title = (!trimmed.is_empty()).then(|| Self::truncate_tab_title(trimmed));
        thread.updated_at_ms = now_unix_ms();
        self.sync_persisted_agent_workspace();
        self.cancel_rename_agent_thread(cx);
    }

    pub(in super::super) fn cancel_rename_agent_thread(&mut self, cx: &mut Context<Self>) {
        if self.renaming_agent_thread_id.take().is_some()
            || !self.agent_thread_rename_input.text().is_empty()
        {
            self.agent_thread_rename_input.clear();
            self.inline_input_selecting = false;
            cx.notify();
        }
    }

    pub(in super::super) fn toggle_agent_project_collapsed(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.collapsed_agent_project_ids.contains(project_id) {
            self.collapsed_agent_project_ids.remove(project_id);
        } else {
            self.collapsed_agent_project_ids
                .insert(project_id.to_string());
            if self.renaming_agent_project_id.as_deref() == Some(project_id) {
                self.cancel_rename_agent_project(cx);
            }
            if self
                .renaming_agent_thread_id
                .as_deref()
                .and_then(|thread_id| self.thread_project_id(thread_id))
                == Some(project_id)
            {
                self.cancel_rename_agent_thread(cx);
            }
        }
        self.sync_persisted_agent_workspace();
        cx.notify();
    }

    pub(in super::super) fn agent_thread_delete_confirm_params(
        thread: &AgentThread,
        display_title: &str,
    ) -> (String, String) {
        let thread_title = display_title;
        let message = if thread.linked_tab_id.is_some() {
            format!(
                "Delete the thread \"{}\" from the sidebar?\n\nThe terminal tab stays open, but it will no longer be tracked as an agent thread.",
                thread_title
            )
        } else {
            format!("Delete the saved thread \"{}\"?", thread_title)
        };
        ("Delete Agent Thread".to_string(), message)
    }

    pub(in super::super) fn agent_project_delete_confirm_params(
        project: &AgentProject,
        thread_count: usize,
    ) -> (String, String) {
        let message = if thread_count == 0 {
            format!(
                "Delete the project \"{}\"?\n\nIts folder reference will be removed from the agent sidebar.",
                project.name
            )
        } else {
            format!(
                "Delete the project \"{}\" and its {} thread(s)?\n\nOpen terminal tabs stay open, but they will no longer be tracked in the agent sidebar.",
                project.name, thread_count
            )
        };
        ("Delete Agent Project".to_string(), message)
    }

    pub(in super::super) fn open_ai_agents_palette_for_project_from_sidebar(
        &mut self,
        project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.command_palette.set_agent_launch_project_id(project_id);
        self.open_command_palette_in_mode(command_palette::CommandPaletteMode::Agents, cx);
    }

    pub(in super::super) fn schedule_agent_project_context_menu(
        &mut self,
        project_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some((project_pinned, project_is_collapsed, git_panel_visible)) = self
            .agent_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| {
                (
                    project.pinned,
                    self.collapsed_agent_project_ids
                        .contains(project.id.as_str()),
                    self.agent_git_panel_matches_target_path(project.root_path.as_str()),
                )
            })
        else {
            return;
        };

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(move || {
                termy_native_sdk::show_agent_project_context_menu(
                    project_pinned,
                    project_is_collapsed,
                    git_panel_visible,
                )
            })
            .await;
            let Some(action) = action else {
                return;
            };
            match action {
                termy_native_sdk::AgentProjectContextMenuAction::NewSession => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.open_ai_agents_palette_for_project_from_sidebar(
                                Some(project_id.clone()),
                                cx,
                            );
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::RenameProject => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.begin_rename_agent_project(project_id.as_str(), cx);
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::ToggleGitPanel => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.toggle_agent_git_panel_for_project(project_id.as_str(), cx);
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::Pin => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let _ = view.set_agent_project_pinned(project_id.as_str(), true, cx);
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::Unpin => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let _ = view.set_agent_project_pinned(project_id.as_str(), false, cx);
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::RevealProject => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match view.reveal_agent_project(project_id.as_str()) {
                                Ok(()) => {
                                    termy_toast::success("Revealed project folder");
                                    view.notify_overlay(cx);
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::CopyPath => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match view.copy_agent_project_path(project_id.as_str(), cx) {
                                Ok(()) => {
                                    termy_toast::success("Copied project path");
                                    view.notify_overlay(cx);
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::CollapseProject => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            if !view
                                .collapsed_agent_project_ids
                                .contains(project_id.as_str())
                            {
                                view.toggle_agent_project_collapsed(project_id.as_str(), cx);
                            }
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::ExpandProject => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            if view
                                .collapsed_agent_project_ids
                                .contains(project_id.as_str())
                            {
                                view.toggle_agent_project_collapsed(project_id.as_str(), cx);
                            }
                        })
                    });
                }
                termy_native_sdk::AgentProjectContextMenuAction::DeleteProject => {
                    let confirm_params = cx.update(|cx| {
                        this.update(cx, |view, _cx| {
                            let project = view
                                .agent_projects
                                .iter()
                                .find(|project| project.id == project_id)?;
                            let thread_count = view.project_thread_count(project_id.as_str());
                            Some(Self::agent_project_delete_confirm_params(
                                project,
                                thread_count,
                            ))
                        })
                        .ok()
                        .flatten()
                    });
                    let Some((title, message)) = confirm_params else {
                        return;
                    };
                    let confirmed =
                        smol::unblock(move || termy_native_sdk::confirm(&title, &message)).await;
                    if !confirmed {
                        return;
                    }
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let project_name = view
                                .agent_projects
                                .iter()
                                .find(|p| p.id == project_id)
                                .map(|p| p.name.clone());
                            match view.delete_agent_project(project_id.as_str()) {
                                Ok(_) => {
                                    termy_toast::success(format!(
                                        "Deleted project \"{}\"",
                                        project_name.unwrap_or_default()
                                    ));
                                    view.notify_overlay(cx);
                                    cx.notify();
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        })
                    });
                }
            }
        })
        .detach();
    }

    pub(in super::super) fn schedule_agent_thread_context_menu(
        &mut self,
        thread_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some((has_live_session, thread_pinned, git_panel_visible)) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| {
                (
                    self.agent_thread_has_live_session(thread),
                    thread.pinned,
                    self.agent_git_panel_matches_target_path(thread.working_dir.as_str()),
                )
            })
        else {
            return;
        };

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let action = smol::unblock(move || {
                termy_native_sdk::show_agent_thread_context_menu(
                    has_live_session,
                    thread_pinned,
                    git_panel_visible,
                )
            })
            .await;
            let Some(action) = action else {
                return;
            };
            match action {
                termy_native_sdk::AgentThreadContextMenuAction::RestartSession => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match view.restart_agent_thread_session(thread_id.as_str(), cx) {
                                Ok(()) => {
                                    termy_toast::success("Restarted agent session");
                                    view.notify_overlay(cx);
                                    cx.notify();
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::CloseSession => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            match view.close_agent_thread_session(thread_id.as_str(), cx) {
                                Ok(()) => {
                                    termy_toast::success("Closed agent session");
                                    view.notify_overlay(cx);
                                    cx.notify();
                                }
                                Err(error) => {
                                    termy_toast::error(error);
                                    view.notify_overlay(cx);
                                }
                            }
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::RenameThread => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.begin_rename_agent_thread(thread_id.as_str(), cx);
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::ToggleGitPanel => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.toggle_agent_git_panel_for_thread(thread_id.as_str(), cx);
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::Pin => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let _ = view.set_agent_thread_pinned(thread_id.as_str(), true, cx);
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::Unpin => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let _ = view.set_agent_thread_pinned(thread_id.as_str(), false, cx);
                        })
                    });
                }
                termy_native_sdk::AgentThreadContextMenuAction::DeleteThread => {
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.request_delete_agent_thread(thread_id.as_str(), cx);
                        })
                    });
                }
            }
        })
        .detach();
    }

    pub(in super::super) fn request_delete_agent_thread(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(confirm_params) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| {
                let display_title = self.agent_thread_display_title(thread);
                Self::agent_thread_delete_confirm_params(thread, &display_title)
            })
        else {
            return;
        };

        let thread_id = thread_id.to_string();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let (title, message) = confirm_params;
            let confirmed =
                smol::unblock(move || termy_native_sdk::confirm(&title, &message)).await;
            if !confirmed {
                return;
            }

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match view.delete_agent_thread(thread_id.as_str()) {
                        Ok(()) => {
                            termy_toast::success("Deleted agent thread");
                            view.notify_overlay(cx);
                            cx.notify();
                        }
                        Err(error) => {
                            termy_toast::error(error);
                            view.notify_overlay(cx);
                        }
                    }
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn close_agent_thread_session(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(thread) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
        else {
            return Err("Agent thread no longer exists".to_string());
        };

        let Some(tab_id) = thread.linked_tab_id else {
            return Err("This thread has no open session to close".to_string());
        };

        let Some(tab_index) = self.tab_index_by_id(tab_id) else {
            self.detach_agent_thread_from_live_tab(thread_id);
            self.sync_persisted_agent_workspace();
            return Err("This thread's session is no longer open".to_string());
        };

        if self.tabs.get(tab_index).is_some_and(|tab| tab.pinned) {
            return Err("Pinned tabs must be unpinned before closing".to_string());
        }

        if self.runtime_kind() == RuntimeKind::Native && self.tabs.len() <= 1 {
            return Err("Can't close the only open tab".to_string());
        }

        self.close_tab(tab_index, cx);
        Ok(())
    }

    pub(in super::super) fn restart_agent_thread_session(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(thread_index) = self
            .agent_threads
            .iter()
            .position(|thread| thread.id == thread_id)
        else {
            return Err("Agent thread no longer exists".to_string());
        };

        if let Some(tab_index) = self.agent_threads[thread_index]
            .linked_tab_id
            .and_then(|tab_id| self.tab_index_by_id(tab_id))
        {
            if self.tabs.get(tab_index).is_some_and(|tab| tab.pinned) {
                return Err("Pinned tabs must be unpinned before restarting".to_string());
            }

            if self.runtime_kind() == RuntimeKind::Native && self.tabs.len() <= 1 {
                if let Some((thread_id, title, current_command, status_label, status_detail)) =
                    self.agent_thread_archive_snapshot_for_tab(tab_index)
                {
                    self.archive_agent_thread_snapshot(
                        thread_id.as_deref(),
                        title.as_str(),
                        current_command.as_deref(),
                        status_label.as_deref(),
                        status_detail.as_deref(),
                    );
                }
                if let Some(tab) = self.tabs.get_mut(tab_index)
                    && tab.agent_thread_id.as_deref() == Some(thread_id)
                {
                    tab.agent_thread_id = None;
                }
            } else {
                self.close_tab(tab_index, cx);
            }
        }

        self.resume_saved_agent_thread(thread_id, cx)
    }

    pub(in super::super) fn copy_agent_project_path(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(project_path) = self
            .agent_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.root_path.clone())
        else {
            return Err("Agent project no longer exists".to_string());
        };

        cx.write_to_clipboard(ClipboardItem::new_string(project_path));
        Ok(())
    }

    pub(in super::super) fn reveal_agent_project(
        &mut self,
        project_id: &str,
    ) -> Result<(), String> {
        let Some(project_path) = self
            .agent_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.root_path.clone())
        else {
            return Err("Agent project no longer exists".to_string());
        };

        let path = Path::new(&project_path);
        if !path.exists() {
            return Err(format!("Project path '{}' no longer exists", project_path));
        }

        #[cfg(target_os = "macos")]
        let status = Command::new("open").arg("-R").arg(path).status();
        #[cfg(target_os = "linux")]
        let status = Command::new("xdg-open").arg(path).status();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let status = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Reveal is unsupported on this platform",
        ));

        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("Reveal command exited with status {}", status)),
            Err(error) => Err(format!("Failed to reveal project path: {}", error)),
        }
    }
}
