use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunShellToolCall {
    pub command: String,
}

pub fn parse_run_shell_tool_call(text: &str) -> Option<RunShellToolCall> {
    let trimmed = text.trim();
    let payload = trimmed
        .strip_prefix("```json")
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    let parsed = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    if parsed.get("tool")?.as_str()? != "run_shell" {
        return None;
    }
    let command = parsed.get("command")?.as_str()?.trim();
    if command.is_empty() {
        return None;
    }

    Some(RunShellToolCall {
        command: command.to_string(),
    })
}

pub fn execute_run_shell_tool(cwd: &Path, command: &str) -> Result<String, String> {
    if command.trim().is_empty() {
        return Err("Empty tool command".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut process = Command::new("cmd");
        process.arg("/C").arg(command);
        process
    };

    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string());
        let mut process = Command::new(shell);
        process.arg("-lc").arg(command);
        process
    };

    let mut output_prefix = String::new();
    if cwd.exists() {
        cmd.current_dir(cwd);
    } else {
        output_prefix.push_str(&format!(
            "[tool] Requested cwd not found, using process cwd instead: {}\n\n",
            cwd.display()
        ));
    }

    let output = cmd
        .output()
        .map_err(|error| format!("tool command failed to start: {error}"))?;

    let mut text = String::new();
    if !output.stdout.is_empty() {
        text.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if text.trim().is_empty() {
        text = "(no output)".to_string();
    }

    let status = output.status.code().unwrap_or_default();
    let mut combined = format!("{output_prefix}$ {command}\n{text}\n(exit code: {status})");
    const MAX_TOOL_OUTPUT_CHARS: usize = 8000;
    if combined.chars().count() > MAX_TOOL_OUTPUT_CHARS {
        combined = combined
            .chars()
            .take(MAX_TOOL_OUTPUT_CHARS)
            .collect::<String>()
            + "\n... (truncated)";
    }

    Ok(combined)
}
