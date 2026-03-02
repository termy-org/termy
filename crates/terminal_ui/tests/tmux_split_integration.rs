#![cfg(unix)]

mod support;

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use support::tmux_harness::{tmux_preflight, tmux_test_binary, tmux_test_guard};

use termy_terminal_ui::{
    TmuxClient, TmuxLaunchTarget, TmuxRuntimeConfig, TmuxSnapshot, TmuxWindowState,
};

const TEST_COLS: u16 = 149;
const TEST_ROWS: u16 = 39;
const TEST_SOCKET_NAME: &str = "termy";

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

fn assert_tmux_test_socket_command_succeeds(binary: &str, args: &[&str], context: &str) {
    let output = run_tmux_test_socket_output(binary, args);
    assert!(
        output.status.success(),
        "{context} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn tmux_client_count(binary: &str) -> usize {
    let output = run_tmux_test_socket_output(
        binary,
        &["list-clients", "-F", "#{client_pid}\t#{client_name}"],
    );
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
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
    const DEFAULT_TMUX_SETTLE_MS: u64 = 750;
    let settle_ms = match env::var("TMUX_SETTLE_MS") {
        Ok(value) => value.parse::<u64>().unwrap_or_else(|_| {
            panic!("TMUX_SETTLE_MS must be an integer millisecond value, got '{value}'")
        }),
        Err(env::VarError::NotPresent) => DEFAULT_TMUX_SETTLE_MS,
        Err(env::VarError::NotUnicode(_)) => panic!("TMUX_SETTLE_MS must be valid UTF-8"),
    };
    thread::sleep(Duration::from_millis(settle_ms));
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
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
fn tmux_new_window_after_inserts_immediately_after_target_window() {
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
    tmux_preflight(binary.as_str());

    let client = new_tmux_client(binary.as_str());
    let session_name = client.session_name().to_string();

    assert_tmux_test_socket_command_succeeds(
        binary.as_str(),
        &["new-window", "-d", "-t", session_name.as_str()],
        "seed second window",
    );
    assert_tmux_test_socket_command_succeeds(
        binary.as_str(),
        &["new-window", "-d", "-t", session_name.as_str()],
        "seed third window",
    );

    let seeded_snapshot = client
        .refresh_snapshot()
        .expect("seeded snapshot should parse");
    assert_eq!(
        seeded_snapshot.windows.len(),
        3,
        "expected 3 windows after seeding, got {}",
        seeded_snapshot.windows.len()
    );

    let middle_window_id = seeded_snapshot.windows[1].id.clone();
    client
        .select_window(middle_window_id.as_str())
        .expect("selecting middle window should succeed");

    let before_insert = client
        .refresh_snapshot()
        .expect("snapshot before insert-after should parse");
    let middle_position_before = before_insert
        .windows
        .iter()
        .position(|window| window.id == middle_window_id)
        .expect("middle window should exist before insert-after");
    let right_neighbor_before = before_insert
        .windows
        .get(middle_position_before + 1)
        .map(|window| window.id.clone())
        .expect("middle window must have a right neighbor in seeded layout");

    client
        .new_window_after(middle_window_id.as_str())
        .expect("insert-after target window should succeed");

    let after_insert = client
        .refresh_snapshot()
        .expect("snapshot after insert-after should parse");
    assert_eq!(
        after_insert.windows.len(),
        before_insert.windows.len() + 1,
        "insert-after should add exactly one window"
    );

    let inserted_window = active_window(&after_insert);
    let inserted_position = after_insert
        .windows
        .iter()
        .position(|window| window.id == inserted_window.id)
        .expect("active inserted window should exist after insert-after");
    let middle_position_after = after_insert
        .windows
        .iter()
        .position(|window| window.id == middle_window_id)
        .expect("middle target window should remain after insert-after");

    assert_eq!(
        inserted_position,
        middle_position_after + 1,
        "inserted window should be immediately right of target window"
    );

    let right_neighbor_after = after_insert
        .windows
        .get(inserted_position + 1)
        .map(|window| window.id.as_str());
    assert_eq!(
        right_neighbor_after,
        Some(right_neighbor_before.as_str()),
        "existing right neighbor should shift right by one slot"
    );
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn tmux_capture_full_rejoins_wrapped_input_rows() {
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
        .capture_pane(pane_id.as_str(), 10_000)
        .expect("capture-pane full history should succeed");
    let capture_text = String::from_utf8_lossy(&capture);

    assert!(
        capture_text.contains("abcdefghijklmnopqrstuvwxyz"),
        "capture should include typed input: '{}'",
        capture_text
    );
    assert!(
        capture_text.contains(wrapped_input),
        "capture should rejoin wrapped rows in full-history mode: '{}'",
        capture_text
    );
}

#[test]
#[ignore = "requires local tmux 3.3+; run explicitly"]
fn managed_nonpersistent_drop_kills_session() {
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
    let binary = tmux_test_binary();
    let _guard = tmux_test_guard(binary.as_str());
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
