#![cfg(unix)]

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use termy_terminal_ui::{
    TmuxClient, TmuxLaunchTarget, TmuxRuntimeConfig, TmuxSnapshot, TmuxWindowState,
};

const TEST_COLS: u16 = 149;
const TEST_ROWS: u16 = 39;

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
socket_name=\"\"\n\
session_name=\"\"\n\
for ((i=0; i<${#args[@]}; i++)); do\n\
  arg=\"${args[$i]}\"\n\
  if [[ \"$arg\" == \"-L\" && $((i+1)) -lt ${#args[@]} ]]; then\n\
    socket_name=\"${args[$((i+1))]}\"\n\
  fi\n\
  if [[ \"$arg\" == \"-CC\" && $((i+1)) -lt ${#args[@]} && \"${args[$((i+1))]}\" == \"new-session\" ]]; then\n\
    is_control_start=1\n\
  fi\n\
  if [[ \"$arg\" == \"-A\" ]]; then\n\
    has_attach=1\n\
  fi\n\
  if [[ \"$arg\" == \"-s\" && $((i+1)) -lt ${#args[@]} ]]; then\n\
    session_name=\"${args[$((i+1))]}\"\n\
  fi\n\
done\n\
if [[ $is_control_start -eq 1 ]]; then\n\
  if [[ -n \"$socket_name\" && -n \"$session_name\" ]]; then\n\
    tmux -L \"$socket_name\" -f /dev/null new-session -d -s \"$session_name\" >/dev/null 2>&1 || true\n\
  fi\n\
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

fn ensure_isolated_tmux_tmpdir() {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let short_suffix = now_ns % 1_000_000;
    let tmux_tmpdir = PathBuf::from("/tmp").join(format!("ttmx-{}-{short_suffix}", process::id()));
    std::fs::create_dir_all(&tmux_tmpdir).expect("failed to create isolated TMUX_TMPDIR");

    // Keep integration tests isolated from any user/server state that might
    // also use `-L termy` by forcing tmux socket files into a dedicated tmpdir.
    // Also clear nested-session hints so local tests behave consistently even
    // when invoked from inside another tmux session.
    unsafe { env::remove_var("TMUX") };
    unsafe { env::set_var("TMUX_TMPDIR", &tmux_tmpdir) };
}

fn new_tmux_client(binary: &str) -> TmuxClient {
    let mut last_error = None::<String>;

    for _attempt in 0..6 {
        let config = TmuxRuntimeConfig {
            binary: binary.to_string(),
            launch: TmuxLaunchTarget::Managed { persistence: true },
        };
        match TmuxClient::new(config, TEST_COLS, TEST_ROWS, None) {
            Ok(client) => return client,
            Err(error) => {
                last_error = Some(format!("{error:#}"));
                // The control socket/session can be briefly unavailable right
                // after spawn on busy systems; keep retries bounded and explicit.
                thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }

    panic!(
        "failed to start tmux client for integration test using '{}': {}",
        binary,
        last_error.unwrap_or_else(|| "unknown startup failure".to_string())
    );
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
    ensure_isolated_tmux_tmpdir();
    let binary = tmux_test_binary();
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
    ensure_isolated_tmux_tmpdir();
    let binary = tmux_test_binary();
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
