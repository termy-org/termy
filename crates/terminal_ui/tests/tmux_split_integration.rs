#![cfg(unix)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::{self, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use termy_terminal_ui::{
    TmuxClient, TmuxLaunchTarget, TmuxRuntimeConfig, TmuxSnapshot, TmuxWindowState,
};

const TEST_COLS: u16 = 149;
const TEST_ROWS: u16 = 39;
const TEST_SOCKET_NAME: &str = "termy";

fn tmux_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tmux_test_binary() -> String {
    if let Ok(binary) = env::var("TERMY_TEST_TMUX_BIN") {
        return binary;
    }

    static WRAPPER_PATH: OnceLock<PathBuf> = OnceLock::new();
    let path = WRAPPER_PATH.get_or_init(|| {
        let path = PathBuf::from("/tmp").join(format!("tmux-e2e-wrapper-{}", process::id()));
        let script = "\
#!/usr/bin/env bash\n\
set -euo pipefail\n\
args=(\"$@\")\n\
is_control_start=0\n\
has_attach=0\n\
for ((i=0; i<${#args[@]}; i++)); do\n\
  arg=\"${args[$i]}\"\n\
  if [[ \"$arg\" == \"-CC\" && $((i+1)) -lt ${#args[@]} && \"${args[$((i+1))]}\" == \"new-session\" ]]; then\n\
    is_control_start=1\n\
  fi\n\
  if [[ \"$arg\" == \"-A\" ]]; then\n\
    has_attach=1\n\
  fi\n\
done\n\
if [[ $is_control_start -eq 1 ]]; then\n\
  if [[ $has_attach -eq 0 ]]; then\n\
    args+=(\"-A\")\n\
  fi\n\
fi\n\
exec tmux -f /dev/null \"${args[@]}\"\n";
        fs::write(&path, script).expect("failed to write tmux test wrapper");
        let mut permissions = fs::metadata(&path)
            .expect("failed to stat tmux test wrapper")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions).expect("failed to chmod tmux test wrapper");
        path
    });

    path.to_string_lossy().into_owned()
}

fn tmux_preflight(binary: &str) {
    TmuxClient::verify_tmux_version(binary, 3, 3).unwrap_or_else(|error| {
        panic!(
            "tmux integration preflight failed for binary '{}': {error}",
            binary
        )
    });
}

fn kill_test_server(binary: &str, tmux_tmpdir: &Path) {
    let _ = Command::new(binary)
        .env_remove("TMUX")
        .env("TMUX_TMPDIR", tmux_tmpdir)
        .arg("-L")
        .arg(TEST_SOCKET_NAME)
        .arg("kill-server")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

struct IsolatedTmuxEnvGuard {
    previous_tmux: Option<OsString>,
    previous_tmux_tmpdir: Option<OsString>,
    tmux_tmpdir: PathBuf,
    binary: String,
}

impl IsolatedTmuxEnvGuard {
    fn new(binary: &str) -> Self {
        let previous_tmux = env::var_os("TMUX");
        let previous_tmux_tmpdir = env::var_os("TMUX_TMPDIR");

        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let short_suffix = now_ns % 1_000_000;
        let tmux_tmpdir = PathBuf::from("/tmp").join(format!("ttmx-{}-{short_suffix}", process::id()));
        fs::create_dir_all(&tmux_tmpdir).expect("failed to create isolated TMUX_TMPDIR");

        // Keep integration tests isolated from user/session state that may also
        // use `-L termy`. Clearing TMUX avoids nested-session hints from parent shells.
        unsafe { env::remove_var("TMUX") };
        unsafe { env::set_var("TMUX_TMPDIR", &tmux_tmpdir) };

        Self {
            previous_tmux,
            previous_tmux_tmpdir,
            tmux_tmpdir,
            binary: binary.to_string(),
        }
    }
}

impl Drop for IsolatedTmuxEnvGuard {
    fn drop(&mut self) {
        kill_test_server(self.binary.as_str(), self.tmux_tmpdir.as_path());
        let _ = fs::remove_dir_all(&self.tmux_tmpdir);

        if let Some(previous_tmux) = self.previous_tmux.take() {
            unsafe { env::set_var("TMUX", previous_tmux) };
        } else {
            unsafe { env::remove_var("TMUX") };
        }

        if let Some(previous_tmux_tmpdir) = self.previous_tmux_tmpdir.take() {
            unsafe { env::set_var("TMUX_TMPDIR", previous_tmux_tmpdir) };
        } else {
            unsafe { env::remove_var("TMUX_TMPDIR") };
        }
    }
}

fn isolated_tmux_tmpdir_from_env() -> PathBuf {
    PathBuf::from(
        env::var_os("TMUX_TMPDIR")
            .expect("TMUX_TMPDIR must be set by IsolatedTmuxEnvGuard before launching tmux tests"),
    )
}

fn ensure_clean_test_server(binary: &str) {
    let tmux_tmpdir = isolated_tmux_tmpdir_from_env();
    kill_test_server(binary, tmux_tmpdir.as_path());
}

fn ensure_isolated_tmux_tmpdir(binary: &str) -> IsolatedTmuxEnvGuard {
    IsolatedTmuxEnvGuard::new(binary)
}

fn run_tmux_test_socket_output(binary: &str, args: &[&str]) -> std::process::Output {
    Command::new(binary)
        .env_remove("TMUX")
        .env("TMUX_TMPDIR", isolated_tmux_tmpdir_from_env())
        .arg("-L")
        .arg(TEST_SOCKET_NAME)
        .args(args)
        .output()
        .expect("failed to execute tmux command for test socket")
}

fn tmux_client_count(binary: &str) -> usize {
    let output = run_tmux_test_socket_output(binary, &["list-clients", "-F", "#{client_pid}\t#{client_name}"]);
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout.lines().filter(|line| !line.trim().is_empty()).count();
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.contains("no current client") || stderr.contains("no server running on") {
        return 0;
    }
    panic!("list-clients failed for test socket: {}", stderr);
}

fn tmux_has_session(binary: &str, session_name: &str) -> bool {
    let output = run_tmux_test_socket_output(binary, &["has-session", "-t", session_name]);
    if output.status.success() {
        return true;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.contains("can't find session") || stderr.contains("no server running on") {
        return false;
    }
    panic!(
        "has-session failed for test socket and session '{}': {}",
        session_name, stderr
    );
}

fn wait_for_tmux_settle() {
    thread::sleep(Duration::from_millis(180));
}

fn new_tmux_client_with_persistence_clean(binary: &str, persistence: bool) -> TmuxClient {
    ensure_clean_test_server(binary);
    new_tmux_client_with_persistence_live(binary, persistence)
}

fn new_tmux_client_with_persistence_live(binary: &str, persistence: bool) -> TmuxClient {
    let mut last_error = None::<String>;

    for _attempt in 0..6 {
        let config = TmuxRuntimeConfig {
            binary: binary.to_string(),
            launch: TmuxLaunchTarget::Managed { persistence },
            show_active_pane_border: false,
        };
        match TmuxClient::new(config, TEST_COLS, TEST_ROWS, None) {
            Ok(client) => return client,
            Err(error) => {
                last_error = Some(format!("{error:#}"));
                // Live reconnect/handoff tests require a stable server/session.
                // Keep retries bounded but do not reset socket state here.
                thread::sleep(Duration::from_millis(120));
            }
        }
    }

    panic!(
        "failed to start tmux client for integration test using '{}': {}",
        binary,
        last_error.unwrap_or_else(|| "unknown startup failure".to_string())
    );
}

fn new_tmux_client(binary: &str) -> TmuxClient {
    new_tmux_client_with_persistence_clean(binary, true)
}

fn active_window(snapshot: &TmuxSnapshot) -> &TmuxWindowState {
    snapshot
        .windows
        .iter()
        .find(|window| window.is_active)
        .expect("snapshot must contain an active window")
}

fn active_pane_id(window: &TmuxWindowState) -> &str {
    window
        .panes
        .iter()
        .find(|pane| pane.is_active)
        .map(|pane| pane.id.as_str())
        .expect("active window must contain an active pane")
}

fn assert_window_geometry_within_bounds(window: &TmuxWindowState, cols: u16, rows: u16) {
    let max_cols = u32::from(cols);
    let max_rows = u32::from(rows);

    for pane in &window.panes {
        assert!(pane.width > 0, "pane {} width must be non-zero", pane.id);
        assert!(pane.height > 0, "pane {} height must be non-zero", pane.id);

        let right = u32::from(pane.left) + u32::from(pane.width);
        let bottom = u32::from(pane.top) + u32::from(pane.height);

        assert!(
            right <= max_cols,
            "pane {} exceeds width bounds: left={} width={} cols={}",
            pane.id,
            pane.left,
            pane.width,
            cols
        );
        assert!(
            bottom <= max_rows,
            "pane {} exceeds height bounds: top={} height={} rows={}",
            pane.id,
            pane.top,
            pane.height,
            rows
        );
    }
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn tmux_split_vertical_then_horizontal_refresh_snapshot_parses_nested_layout() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client(binary.as_str());

    let initial_snapshot = client
        .refresh_snapshot()
        .expect("initial snapshot should parse");
    let initial_window = active_window(&initial_snapshot);
    let first_target = active_pane_id(initial_window).to_string();

    client
        .split_vertical(first_target.as_str())
        .expect("vertical split should succeed");

    let after_vertical = client
        .refresh_snapshot()
        .expect("snapshot after vertical split should parse");
    let after_vertical_window = active_window(&after_vertical);
    let second_target = active_pane_id(after_vertical_window).to_string();

    client
        .split_horizontal(second_target.as_str())
        .expect("horizontal split should succeed");

    let final_snapshot = client
        .refresh_snapshot()
        .expect("snapshot after nested split should parse");
    let final_window = active_window(&final_snapshot);

    assert!(
        final_window.panes.len() >= 3,
        "expected at least 3 panes after two splits, got {}",
        final_window.panes.len()
    );
    assert!(
        final_window.layout.contains('['),
        "expected nested layout after split sequence, got '{}'",
        final_window.layout
    );

    let active_pane_count = final_window
        .panes
        .iter()
        .filter(|pane| pane.is_active)
        .count();
    assert_eq!(active_pane_count, 1, "expected exactly one active pane");

    assert_window_geometry_within_bounds(final_window, TEST_COLS, TEST_ROWS);
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn tmux_repeated_split_refresh_cycles_remain_parseable() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client(binary.as_str());

    let mut previous_pane_count = active_window(
        &client
            .refresh_snapshot()
            .expect("initial snapshot should parse"),
    )
    .panes
    .len();

    for iteration in 0..4 {
        let before_snapshot = client
            .refresh_snapshot()
            .expect("snapshot before split should parse");
        let before_window = active_window(&before_snapshot);
        let target = active_pane_id(before_window).to_string();

        if iteration % 2 == 0 {
            client
                .split_vertical(target.as_str())
                .expect("vertical split should succeed");
        } else {
            client
                .split_horizontal(target.as_str())
                .expect("horizontal split should succeed");
        }

        let after_snapshot = client
            .refresh_snapshot()
            .expect("snapshot after split should parse");
        let after_window = active_window(&after_snapshot);

        assert!(
            after_window.panes.len() > previous_pane_count,
            "pane count should grow after split: before={} after={}",
            previous_pane_count,
            after_window.panes.len()
        );
        previous_pane_count = after_window.panes.len();

        assert_window_geometry_within_bounds(after_window, TEST_COLS, TEST_ROWS);
    }
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn tmux_capture_viewport_preserves_wrapped_input_rows() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client(binary.as_str());
    client
        .set_client_size(40, 10)
        .expect("client resize should succeed");

    let resized_snapshot = client
        .refresh_snapshot()
        .expect("resized snapshot should parse");
    let resized_window = active_window(&resized_snapshot);
    let pane_id = active_pane_id(resized_window).to_string();
    let pane_width = resized_window
        .panes
        .iter()
        .find(|pane| pane.id == pane_id)
        .map(|pane| pane.width)
        .expect("resized pane should exist");
    assert!(
        pane_width <= 40,
        "expected wrapped-pane width <= 40, got {}",
        pane_width
    );

    let wrapped_input = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    client
        .send_input(pane_id.as_str(), wrapped_input.as_bytes())
        .expect("send-input should succeed");

    let capture = client
        .capture_pane_viewport(pane_id.as_str())
        .expect("capture-pane viewport should succeed");
    let capture_text = String::from_utf8_lossy(&capture);

    assert!(
        capture_text.contains("abcdefghijklmnopqrstuvwxyz"),
        "capture should include typed input: '{}'",
        capture_text
    );
    assert!(
        !capture_text.contains(wrapped_input),
        "capture unexpectedly joined wrapped rows; expected raw viewport wrapping: '{}'",
        capture_text
    );
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn managed_nonpersistent_drop_kills_session() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client_with_persistence_clean(binary.as_str(), false);
    let session_name = client.session_name().to_string();
    assert!(tmux_has_session(binary.as_str(), session_name.as_str()));
    assert_eq!(tmux_client_count(binary.as_str()), 1);

    drop(client);
    wait_for_tmux_settle();

    assert!(
        !tmux_has_session(binary.as_str(), session_name.as_str()),
        "non-persistent managed session '{}' must be torn down on drop",
        session_name
    );
    assert_eq!(
        tmux_client_count(binary.as_str()),
        0,
        "no tmux clients should remain after non-persistent drop"
    );
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn managed_persistent_drop_keeps_session_but_removes_client() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client_with_persistence_clean(binary.as_str(), true);
    let session_name = client.session_name().to_string();
    assert!(tmux_has_session(binary.as_str(), session_name.as_str()));
    assert_eq!(tmux_client_count(binary.as_str()), 1);

    drop(client);
    wait_for_tmux_settle();

    assert!(
        tmux_has_session(binary.as_str(), session_name.as_str()),
        "persistent managed session '{}' must survive drop",
        session_name
    );
    assert_eq!(
        tmux_client_count(binary.as_str()),
        0,
        "persistent drop must not leave control clients attached"
    );
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn repeated_reconnect_does_not_increase_client_count() {
    let _guard = tmux_test_guard();
    let binary = tmux_test_binary();
    let _env_guard = ensure_isolated_tmux_tmpdir(binary.as_str());
    tmux_preflight(binary.as_str());

    let mut client = new_tmux_client_with_persistence_clean(binary.as_str(), true);
    assert_eq!(
        tmux_client_count(binary.as_str()),
        1,
        "baseline must start with one control client"
    );

    for _ in 0..4 {
        let next_client = new_tmux_client_with_persistence_live(binary.as_str(), true);
        assert!(
            tmux_client_count(binary.as_str()) <= 2,
            "handoff should temporarily have at most two clients"
        );
        client
            .shutdown_default()
            .expect("previous client cleanup should succeed during reconnect");
        wait_for_tmux_settle();
        assert_eq!(
            tmux_client_count(binary.as_str()),
            1,
            "reconnect must converge back to exactly one control client"
        );
        client = next_client;
    }

    drop(client);
    wait_for_tmux_settle();

    assert_eq!(
        tmux_client_count(binary.as_str()),
        0,
        "final persistent client drop must detach all control clients"
    );
    assert!(
        tmux_has_session(binary.as_str(), "termy"),
        "persistent session must remain after reconnect loop"
    );
}
