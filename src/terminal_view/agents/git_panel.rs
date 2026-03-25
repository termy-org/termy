use super::*;

impl TerminalView {
    pub(in super::super) fn close_agent_git_panel(&mut self) {
        self.agent_git_panel = AgentGitPanelState::default();
        self.agent_git_panel_input_mode = None;
        self.agent_git_panel_input.clear();
        self.agent_git_panel_branch_dropdown_open = false;
        self.agent_git_panel_poll_task = None;
    }

    pub(in super::super) fn start_agent_git_panel_poll(&mut self, cx: &mut Context<Self>) {
        if self.agent_git_panel_poll_task.is_some() {
            return;
        }
        let task = cx.spawn(async move |this, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(3)).await;
                let still_open = cx
                    .update(|cx| {
                        this.update(cx, |view, cx| {
                            if view.agent_git_panel.open {
                                view.refresh_agent_git_panel(cx);
                                true
                            } else {
                                false
                            }
                        })
                    })
                    .unwrap_or(false);
                if !still_open {
                    break;
                }
            }
        });
        self.agent_git_panel_poll_task = Some(task);
    }

    pub(in super::super) fn cancel_agent_git_panel_input(&mut self, cx: &mut Context<Self>) {
        if self.agent_git_panel_input_mode.take().is_some()
            || !self.agent_git_panel_input.text().is_empty()
        {
            self.agent_git_panel_input.clear();
            self.inline_input_selecting = false;
            cx.notify();
        }
    }

    pub(in super::super) fn begin_agent_git_panel_input(
        &mut self,
        mode: AgentGitPanelInputMode,
        initial_text: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.agent_git_panel_input_mode = Some(mode);
        self.agent_git_panel_input.set_text(initial_text.into());
        self.inline_input_selecting = true;
        cx.notify();
    }

    pub(in super::super) fn agent_git_panel_matches_target_path(&self, path: &str) -> bool {
        if !self.agent_git_panel.open {
            return false;
        }

        if let Some(repo_root) = self.agent_git_panel.repo_root.as_deref() {
            let repo_root = Path::new(repo_root);
            let target = Path::new(path);
            if target == repo_root || target.starts_with(repo_root) {
                return true;
            }
        }

        self.agent_git_panel.source_path.as_deref() == Some(path)
    }

    pub(in super::super) fn run_git_command(
        repo_root: &str,
        args: &[&str],
    ) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .map_err(|error| {
                format!(
                    "Failed to run git {}: {}",
                    args.first().copied().unwrap_or("command"),
                    error
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("git {} failed", args.first().copied().unwrap_or("command"))
            } else {
                stderr
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(in super::super) fn run_git_diff_command(
        repo_root: &str,
        args: &[&str],
    ) -> Result<String, String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .map_err(|error| format!("Failed to run git diff: {}", error))?;
        let code = output.status.code().unwrap_or_default();
        if !(output.status.success() || code == 1) {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "git diff failed".to_string()
            } else {
                stderr
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub(in super::super) fn parse_agent_git_branch_summary(
        branch_line: &str,
    ) -> (Option<String>, usize, usize) {
        let current_branch = branch_line
            .split("...")
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != "HEAD (no branch)")
            .map(str::to_string);
        let mut ahead = 0usize;
        let mut behind = 0usize;
        if let Some(start) = branch_line.find('[')
            && let Some(end_rel) = branch_line[start + 1..].find(']')
        {
            let details = &branch_line[start + 1..start + 1 + end_rel];
            for part in details.split(',') {
                let trimmed = part.trim();
                if let Some(value) = trimmed.strip_prefix("ahead ") {
                    ahead = value.parse::<usize>().unwrap_or_default();
                }
                if let Some(value) = trimmed.strip_prefix("behind ") {
                    behind = value.parse::<usize>().unwrap_or_default();
                }
            }
        }
        (current_branch, ahead, behind)
    }

    pub(in super::super) fn parse_agent_git_history(output: &str) -> Vec<AgentGitHistoryEntry> {
        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| AgentGitHistoryEntry {
                summary: line.to_string(),
            })
            .collect()
    }

    pub(in super::super) fn parse_agent_git_stashes(output: &str) -> Vec<AgentGitStashEntry> {
        output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ' ');
                AgentGitStashEntry {
                    name: parts.next().unwrap_or_default().to_string(),
                    summary: parts.next().unwrap_or_default().to_string(),
                }
            })
            .collect()
    }

    pub(in super::super) fn load_agent_git_panel_snapshot(
        path: &str,
    ) -> Result<AgentGitPanelSnapshot, String> {
        let repo_root = Self::run_git_command(path, &["rev-parse", "--show-toplevel"])?
            .trim()
            .to_string();
        let status_output = Self::run_git_command(
            repo_root.as_str(),
            &[
                "status",
                "--porcelain=v1",
                "--branch",
                "--untracked-files=all",
            ],
        )?;
        let mut branch = None;
        let mut current_branch = None;
        let mut ahead = 0usize;
        let mut behind = 0usize;
        let mut entries = Vec::new();
        for line in status_output.lines() {
            if let Some(branch_line) = line.strip_prefix("## ") {
                branch = Some(branch_line.to_string());
                let parsed = Self::parse_agent_git_branch_summary(branch_line);
                current_branch = parsed.0;
                ahead = parsed.1;
                behind = parsed.2;
                continue;
            }
            if line.len() < 3 {
                continue;
            }
            let status = line.get(0..2).unwrap_or("");
            let path = line.get(3..).unwrap_or("");
            entries.push(AgentGitPanelEntry::from_status_line(status, path));
        }

        let last_commit =
            Self::run_git_command(repo_root.as_str(), &["log", "-1", "--pretty=%h %s"])
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
        let project_history = Self::parse_agent_git_history(&Self::run_git_command(
            repo_root.as_str(),
            &["log", "-n", "8", "--pretty=%h %s"],
        )?);
        let branches = Self::run_git_command(
            repo_root.as_str(),
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        )?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
        let stashes = Self::parse_agent_git_stashes(&Self::run_git_command(
            repo_root.as_str(),
            &["stash", "list", "--format=%gd %s"],
        )?);
        let dirty_count = entries.len();

        Ok(AgentGitPanelSnapshot {
            repo_root,
            branch,
            current_branch,
            ahead,
            behind,
            dirty_count,
            last_commit,
            entries,
            project_history,
            branches,
            stashes,
        })
    }

    pub(in super::super) fn load_agent_git_panel_preview(
        repo_root: &str,
        entry: &AgentGitPanelEntry,
    ) -> Result<AgentGitPanelPreviewSnapshot, String> {
        let diff = if entry.is_untracked() {
            let absolute = Path::new(repo_root).join(entry.repo_path.as_str());
            Self::run_git_diff_command(
                repo_root,
                &[
                    "diff",
                    "--no-index",
                    "--unified=3",
                    "--",
                    "/dev/null",
                    absolute.to_string_lossy().as_ref(),
                ],
            )?
        } else {
            Self::run_git_diff_command(
                repo_root,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--unified=3",
                    "HEAD",
                    "--",
                    entry.repo_path.as_str(),
                ],
            )?
        };
        let history = Self::parse_agent_git_history(&Self::run_git_command(
            repo_root,
            &[
                "log",
                "-n",
                "8",
                "--pretty=%h %s",
                "--",
                entry.repo_path.as_str(),
            ],
        )?);
        Ok(AgentGitPanelPreviewSnapshot {
            diff_lines: diff.lines().map(str::to_string).collect(),
            history,
        })
    }

    pub(in super::super) fn refresh_agent_git_panel(&mut self, cx: &mut Context<Self>) {
        let (Some(source_path), Some(label)) = (
            self.agent_git_panel.source_path.clone(),
            self.agent_git_panel.label.clone(),
        ) else {
            return;
        };

        self.agent_git_panel.open = true;
        let show_loading = self.agent_git_panel.repo_root.is_none();
        if show_loading {
            self.agent_git_panel.loading = true;
        }
        self.agent_git_panel.error = None;
        cx.notify();

        let source_path_for_load = source_path.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || Self::load_agent_git_panel_snapshot(&source_path_for_load))
                    .await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if !view.agent_git_panel.open
                        || view.agent_git_panel.source_path.as_deref() != Some(source_path.as_str())
                    {
                        return;
                    }

                    view.agent_git_panel.loading = false;
                    match result {
                        Ok(snapshot) => {
                            let selected = view.agent_git_panel.selected_repo_path.clone();
                            view.agent_git_panel.repo_root = Some(snapshot.repo_root);
                            view.agent_git_panel.branch = snapshot.branch;
                            view.agent_git_panel.current_branch = snapshot.current_branch;
                            view.agent_git_panel.ahead = snapshot.ahead;
                            view.agent_git_panel.behind = snapshot.behind;
                            view.agent_git_panel.dirty_count = snapshot.dirty_count;
                            view.agent_git_panel.last_commit = snapshot.last_commit;
                            view.agent_git_panel.project_history = snapshot.project_history;
                            view.agent_git_panel.branches = snapshot.branches;
                            view.agent_git_panel.stashes = snapshot.stashes;
                            view.agent_git_panel.error = None;
                            view.agent_git_panel.entries = snapshot.entries;
                            if let Some(selected_path) = selected {
                                if view
                                    .agent_git_panel
                                    .entries
                                    .iter()
                                    .any(|entry| entry.repo_path == selected_path)
                                {
                                    view.select_agent_git_panel_entry(selected_path.as_str(), cx);
                                } else {
                                    view.clear_agent_git_panel_preview();
                                }
                            }
                        }
                        Err(error) => {
                            view.agent_git_panel.repo_root = None;
                            view.agent_git_panel.branch = None;
                            view.agent_git_panel.current_branch = None;
                            view.agent_git_panel.ahead = 0;
                            view.agent_git_panel.behind = 0;
                            view.agent_git_panel.dirty_count = 0;
                            view.agent_git_panel.last_commit = None;
                            view.agent_git_panel.project_history.clear();
                            view.agent_git_panel.branches.clear();
                            view.agent_git_panel.stashes.clear();
                            view.agent_git_panel.entries.clear();
                            view.agent_git_panel.error = Some(error);
                            view.clear_agent_git_panel_preview();
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
        let _ = label;
        self.start_agent_git_panel_poll(cx);
    }

    pub(in super::super) fn clear_agent_git_panel_preview(&mut self) {
        self.agent_git_panel.selected_repo_path = None;
        self.agent_git_panel.preview_loading = false;
        self.agent_git_panel.preview_error = None;
        self.agent_git_panel.preview_diff_lines.clear();
        self.agent_git_panel.preview_history.clear();
    }

    pub(in super::super) fn select_agent_git_panel_entry(
        &mut self,
        repo_path: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.agent_git_panel.repo_root.clone() else {
            return;
        };
        let Some(entry) = self
            .agent_git_panel
            .entries
            .iter()
            .find(|entry| entry.repo_path == repo_path)
            .cloned()
        else {
            return;
        };

        if self.agent_git_panel.selected_repo_path.as_deref() == Some(repo_path)
            && !self.agent_git_panel.preview_loading
        {
            self.clear_agent_git_panel_preview();
            cx.notify();
            return;
        }

        self.agent_git_panel.selected_repo_path = Some(repo_path.to_string());
        self.agent_git_panel.preview_loading = true;
        self.agent_git_panel.preview_error = None;
        self.agent_git_panel.preview_diff_lines.clear();
        self.agent_git_panel.preview_history.clear();
        cx.notify();

        let selected_repo_path = repo_path.to_string();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || Self::load_agent_git_panel_preview(&repo_root, &entry)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if view.agent_git_panel.selected_repo_path.as_deref()
                        != Some(selected_repo_path.as_str())
                    {
                        return;
                    }
                    view.agent_git_panel.preview_loading = false;
                    match result {
                        Ok(preview) => {
                            view.agent_git_panel.preview_error = None;
                            view.agent_git_panel.preview_diff_lines = preview.diff_lines;
                            view.agent_git_panel.preview_history = preview.history;
                        }
                        Err(error) => {
                            view.agent_git_panel.preview_error = Some(error);
                            view.agent_git_panel.preview_diff_lines.clear();
                            view.agent_git_panel.preview_history.clear();
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn open_agent_git_panel_for_path(
        &mut self,
        source_path: &str,
        label: String,
        cx: &mut Context<Self>,
    ) {
        self.agent_git_panel.open = true;
        self.agent_git_panel.source_path = Some(source_path.to_string());
        self.agent_git_panel.label = Some(label);
        self.agent_git_panel.selected_repo_path = None;
        self.agent_git_panel.preview_loading = false;
        self.agent_git_panel.preview_error = None;
        self.agent_git_panel.preview_diff_lines.clear();
        self.agent_git_panel.preview_history.clear();
        self.refresh_agent_git_panel(cx);
    }

    pub(in super::super) fn toggle_agent_git_panel_for_path(
        &mut self,
        source_path: &str,
        label: String,
        cx: &mut Context<Self>,
    ) {
        if self.agent_git_panel_matches_target_path(source_path) {
            self.close_agent_git_panel();
            cx.notify();
            return;
        }
        self.open_agent_git_panel_for_path(source_path, label, cx);
    }

    pub(in super::super) fn toggle_agent_git_panel_for_project(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .agent_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| (project.root_path.clone(), project.name.clone()))
        else {
            return;
        };

        self.toggle_agent_git_panel_for_path(
            project.0.as_str(),
            format!("Project · {}", project.1),
            cx,
        );
    }

    pub(in super::super) fn toggle_agent_git_panel_for_thread(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some((working_dir, title)) = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| {
                (
                    thread.working_dir.clone(),
                    self.agent_thread_display_title(thread),
                )
            })
        else {
            return;
        };

        self.toggle_agent_git_panel_for_path(
            working_dir.as_str(),
            format!("Thread · {}", title),
            cx,
        );
    }

    pub(in super::super) fn agent_git_entries_for_filter(&self) -> Vec<AgentGitPanelEntry> {
        self.agent_git_panel
            .entries
            .iter()
            .filter(|entry| match self.agent_git_panel.filter {
                AgentGitPanelFilter::All => true,
                AgentGitPanelFilter::Staged => entry.is_staged(),
                AgentGitPanelFilter::Unstaged => entry.is_unstaged(),
                AgentGitPanelFilter::Untracked => entry.is_untracked(),
            })
            .cloned()
            .collect()
    }

    pub(in super::super) fn set_agent_git_panel_filter(
        &mut self,
        filter: AgentGitPanelFilter,
        cx: &mut Context<Self>,
    ) {
        if self.agent_git_panel.filter == filter {
            return;
        }
        self.agent_git_panel.filter = filter;
        cx.notify();
    }

    pub(in super::super) fn open_agent_git_file(&self, repo_path: &str) -> Result<(), String> {
        let repo_root = self
            .agent_git_panel
            .repo_root
            .as_deref()
            .ok_or_else(|| "Git panel is not ready".to_string())?;
        let path = Path::new(repo_root).join(repo_path);

        #[cfg(target_os = "macos")]
        let status = Command::new("open").arg(&path).status();
        #[cfg(target_os = "linux")]
        let status = Command::new("xdg-open").arg(&path).status();
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let status = Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Opening files is unsupported on this platform",
        ));

        status
            .map_err(|error| format!("Failed to open '{}': {}", path.display(), error))?
            .success()
            .then_some(())
            .ok_or_else(|| format!("Failed to open '{}'", path.display()))
    }

    pub(in super::super) fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    pub(in super::super) fn open_agent_git_full_diff(
        &mut self,
        repo_path: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let repo_root = self
            .agent_git_panel
            .repo_root
            .clone()
            .ok_or_else(|| "Git panel is not ready".to_string())?;
        let command = if self
            .agent_git_panel
            .entries
            .iter()
            .find(|entry| entry.repo_path == repo_path)
            .is_some_and(AgentGitPanelEntry::is_untracked)
        {
            let full_path = Path::new(repo_root.as_str()).join(repo_path);
            format!(
                "git -C {} diff --no-index --unified=20 -- /dev/null {} | less -R\n",
                Self::shell_quote(repo_root.as_str()),
                Self::shell_quote(full_path.to_string_lossy().as_ref())
            )
        } else {
            format!(
                "git -C {} diff --no-ext-diff --unified=20 HEAD -- {} | less -R\n",
                Self::shell_quote(repo_root.as_str()),
                Self::shell_quote(repo_path)
            )
        };

        self.add_tab_with_working_dir(Some(repo_root.as_str()), cx);
        if let Some(tab) = self.tabs.get(self.active_tab)
            && let Some(terminal) = tab.active_terminal()
        {
            terminal.write_input(command.as_bytes());
            cx.notify();
            Ok(())
        } else {
            Err("Failed to open diff tab".to_string())
        }
    }

    pub(in super::super) fn run_agent_git_mutation(
        &mut self,
        args: Vec<String>,
        success_message: &'static str,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.agent_git_panel.repo_root.clone() else {
            termy_toast::error("Git panel is not ready");
            self.notify_overlay(cx);
            return;
        };

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = smol::unblock(move || {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(repo_root.as_str())
                    .args(args.iter().map(String::as_str))
                    .output()
                    .map_err(|error| format!("Failed to run git: {}", error))?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    return Err(if stderr.is_empty() {
                        "Git command failed".to_string()
                    } else {
                        stderr
                    });
                }
                Ok(())
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| match result {
                    Ok(()) => {
                        termy_toast::success(success_message);
                        view.refresh_agent_git_panel(cx);
                        view.notify_overlay(cx);
                    }
                    Err(error) => {
                        termy_toast::error(error);
                        view.notify_overlay(cx);
                    }
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn discard_agent_git_entry(
        &mut self,
        entry: AgentGitPanelEntry,
        cx: &mut Context<Self>,
    ) {
        let Some(repo_root) = self.agent_git_panel.repo_root.clone() else {
            termy_toast::error("Git panel is not ready");
            self.notify_overlay(cx);
            return;
        };

        let repo_path = entry.repo_path.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let title = if entry.is_untracked() {
                "Discard Untracked File".to_string()
            } else {
                "Discard File Changes".to_string()
            };
            let message = format!("Discard changes for '{}' ?", entry.path);
            let confirmed =
                smol::unblock(move || termy_native_sdk::confirm(&title, &message)).await;
            if !confirmed {
                return;
            }

            let result = smol::unblock(move || {
                if entry.is_untracked() {
                    let output = Command::new("git")
                        .arg("-C")
                        .arg(repo_root.as_str())
                        .args(["clean", "-f", "--", repo_path.as_str()])
                        .output()
                        .map_err(|error| format!("Failed to run git clean: {}", error))?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        return Err(if stderr.is_empty() {
                            "git clean failed".to_string()
                        } else {
                            stderr
                        });
                    }
                    return Ok(());
                }

                let output = Command::new("git")
                    .arg("-C")
                    .arg(repo_root.as_str())
                    .args([
                        "restore",
                        "--staged",
                        "--worktree",
                        "--source=HEAD",
                        "--",
                        repo_path.as_str(),
                    ])
                    .output()
                    .map_err(|error| format!("Failed to run git restore: {}", error))?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    return Err(if stderr.is_empty() {
                        "git restore failed".to_string()
                    } else {
                        stderr
                    });
                }
                Ok(())
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| match result {
                    Ok(()) => {
                        termy_toast::success("Discarded file changes");
                        view.refresh_agent_git_panel(cx);
                        view.notify_overlay(cx);
                    }
                    Err(error) => {
                        termy_toast::error(error);
                        view.notify_overlay(cx);
                    }
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn commit_agent_git_panel_input(&mut self, cx: &mut Context<Self>) {
        let Some(mode) = self.agent_git_panel_input_mode else {
            return;
        };
        let value = self.agent_git_panel_input.text().trim().to_string();
        if value.is_empty() {
            termy_toast::error("Input cannot be empty");
            self.notify_overlay(cx);
            return;
        }
        self.cancel_agent_git_panel_input(cx);
        match mode {
            AgentGitPanelInputMode::Commit => {
                self.run_agent_git_mutation(
                    vec!["commit".to_string(), "-m".to_string(), value],
                    "Created commit",
                    cx,
                );
            }
            AgentGitPanelInputMode::CreateBranch => {
                self.run_agent_git_mutation(
                    vec!["checkout".to_string(), "-b".to_string(), value],
                    "Created branch",
                    cx,
                );
            }
            AgentGitPanelInputMode::SaveStash => {
                self.run_agent_git_mutation(
                    vec![
                        "stash".to_string(),
                        "push".to_string(),
                        "-m".to_string(),
                        value,
                    ],
                    "Saved stash",
                    cx,
                );
            }
        }
    }
}
