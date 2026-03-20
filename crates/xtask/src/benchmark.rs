use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use termy_terminal_ui::terminal_ui_monotonic_now_ns;

const DEFAULT_DURATION_SECS: u64 = 13;
const TRACE_PADDING_SECS: u64 = 2;
const BENCHMARK_EVENTS_PATH_ENV: &str = "TERMY_BENCHMARK_EVENTS_PATH";
const IDLE_BURST_PRE_IDLE: Duration = Duration::from_millis(1500);
const IDLE_BURST_POST_IDLE: Duration = Duration::from_millis(1000);
const ECHO_TRAIN_PRE_IDLE: Duration = Duration::from_millis(1500);
const ECHO_TRAIN_INTERVAL: Duration = Duration::from_millis(250);
const ECHO_TRAIN_POST_IDLE: Duration = Duration::from_millis(1000);
const ECHO_TRAIN_DEFAULT_ITERATIONS: u64 = 40;

pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let Some(command) = args.next() else {
        bail!("usage: cargo run -p xtask -- <benchmark-driver|benchmark-compare> [options]");
    };

    match command.as_str() {
        "benchmark-driver" => run_driver(args),
        "benchmark-compare" => run_compare(args),
        other => bail!("unknown benchmark command `{other}`"),
    }
}

fn run_driver(mut args: impl Iterator<Item = String>) -> Result<()> {
    let mut scenario = None;
    let mut duration_secs = DEFAULT_DURATION_SECS;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => {
                let value = args.next().context("missing value for --scenario")?;
                scenario = Some(Scenario::parse(&value)?);
            }
            "--duration-secs" => {
                let value = args.next().context("missing value for --duration-secs")?;
                duration_secs = value
                    .parse()
                    .with_context(|| format!("invalid --duration-secs `{value}`"))?;
            }
            other => bail!(
                "unknown benchmark-driver argument `{other}`; expected --scenario or --duration-secs"
            ),
        }
    }

    let scenario = scenario.context("missing required --scenario")?;
    scenario.run(Duration::from_secs(duration_secs))
}

fn run_compare(mut args: impl Iterator<Item = String>) -> Result<()> {
    let mut baseline_root = None;
    let mut candidate_root = None;
    let mut output_root = None;
    let mut duration_secs = DEFAULT_DURATION_SECS;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--baseline-root" => {
                baseline_root = Some(PathBuf::from(
                    args.next().context("missing value for --baseline-root")?,
                ));
            }
            "--candidate-root" => {
                candidate_root = Some(PathBuf::from(
                    args.next().context("missing value for --candidate-root")?,
                ));
            }
            "--output" => {
                output_root = Some(PathBuf::from(
                    args.next().context("missing value for --output")?,
                ));
            }
            "--duration-secs" => {
                let value = args.next().context("missing value for --duration-secs")?;
                duration_secs = value
                    .parse()
                    .with_context(|| format!("invalid --duration-secs `{value}`"))?;
            }
            other => bail!(
                "unknown benchmark-compare argument `{other}`; expected --baseline-root, --candidate-root, --output, or --duration-secs"
            ),
        }
    }

    let baseline_root = canonicalize_root(baseline_root.context("missing --baseline-root")?)?;
    let candidate_root = canonicalize_root(candidate_root.context("missing --candidate-root")?)?;
    let output_root = output_root.context("missing --output")?;
    if output_root.exists() {
        fs::remove_dir_all(&output_root)
            .with_context(|| format!("failed to clear {}", output_root.display()))?;
    }
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let baseline = BuildSpec::new("baseline", baseline_root)?;
    let candidate = BuildSpec::new("candidate", candidate_root)?;

    build_release_binaries(&baseline)?;
    build_release_binaries(&candidate)?;

    let scenarios = Scenario::all();
    let mut runs = Vec::with_capacity(scenarios.len() * 2);
    for build in [&baseline, &candidate] {
        for scenario in scenarios {
            runs.push(run_single_benchmark(
                build,
                *scenario,
                duration_secs,
                &output_root,
            )?);
        }
    }

    let summary = ComparisonSummary::from_runs(&baseline, &candidate, runs)?;
    write_report_artifacts(&output_root, &summary)?;
    println!("wrote benchmark report to {}", output_root.display());
    Ok(())
}

fn canonicalize_root(path: PathBuf) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))
}

#[derive(Clone, Debug)]
struct BuildSpec {
    label: &'static str,
    root: PathBuf,
    git_sha: String,
}

impl BuildSpec {
    fn new(label: &'static str, root: PathBuf) -> Result<Self> {
        Ok(Self {
            label,
            git_sha: git_rev_parse_short(&root)?,
            root,
        })
    }

    fn termy_binary(&self) -> PathBuf {
        self.root.join("target/release/termy")
    }

    fn xtask_binary(&self) -> PathBuf {
        self.root.join("target/release/xtask")
    }
}

fn git_rev_parse_short(root: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to run git rev-parse in {}", root.display()))?;
    if !output.status.success() {
        bail!(
            "git rev-parse failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_release_binaries(build: &BuildSpec) -> Result<()> {
    run_command(
        Command::new("cargo")
            .arg("build")
            .arg("--release")
            .arg("-p")
            .arg("termy")
            .arg("-p")
            .arg("xtask")
            .current_dir(&build.root),
        format!("cargo build --release in {}", build.root.display()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Scenario {
    IdleBurst,
    EchoTrain,
    SteadyScroll,
    AltScreenAnim,
}

impl Scenario {
    fn all() -> &'static [Scenario] {
        &[
            Scenario::IdleBurst,
            Scenario::EchoTrain,
            Scenario::SteadyScroll,
            Scenario::AltScreenAnim,
        ]
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "idle-burst" => Ok(Self::IdleBurst),
            "echo-train" => Ok(Self::EchoTrain),
            "steady-scroll" => Ok(Self::SteadyScroll),
            "alt-screen-anim" => Ok(Self::AltScreenAnim),
            other => bail!("unknown benchmark scenario `{other}`"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::IdleBurst => "idle-burst",
            Self::EchoTrain => "echo-train",
            Self::SteadyScroll => "steady-scroll",
            Self::AltScreenAnim => "alt-screen-anim",
        }
    }

    fn run(self, duration: Duration) -> Result<()> {
        match self {
            Self::IdleBurst => run_idle_burst(duration),
            Self::EchoTrain => run_echo_train(duration),
            Self::SteadyScroll => run_steady_scroll(duration),
            Self::AltScreenAnim => run_alt_screen_anim(duration),
        }
    }
}

fn run_idle_burst(duration: Duration) -> Result<()> {
    let start = Instant::now();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut marker_writer = BenchmarkMarkerWriter::new_from_env()?;

    sleep_for_remaining(start, duration, IDLE_BURST_PRE_IDLE);
    marker_writer.record("burst_start", None)?;

    let mut burst = String::new();
    for line in 0..16u64 {
        burst.push_str(&format!(
            "burst line {line:02} 0123456789 abcdefghijklmnopqrstuvwxyz\n"
        ));
    }
    out.write_all(burst.as_bytes())?;
    out.flush()?;
    marker_writer.record("burst_end", None)?;

    sleep_for_remaining(start, duration, IDLE_BURST_POST_IDLE);
    marker_writer.flush()?;
    Ok(())
}

fn run_echo_train(duration: Duration) -> Result<()> {
    let start = Instant::now();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut marker_writer = BenchmarkMarkerWriter::new_from_env()?;

    sleep_for_remaining(start, duration, ECHO_TRAIN_PRE_IDLE);

    let remaining = duration.saturating_sub(start.elapsed());
    let reserved_post_idle = ECHO_TRAIN_POST_IDLE.min(remaining);
    let emit_budget = remaining.saturating_sub(reserved_post_idle);
    let budget_iterations = (emit_budget.as_millis() / ECHO_TRAIN_INTERVAL.as_millis()) as u64;
    let iterations = budget_iterations.max(1).min(ECHO_TRAIN_DEFAULT_ITERATIONS);
    let glyphs = b"abcdefghijklmnopqrstuvwxyz0123456789";

    for seq in 0..iterations {
        marker_writer.record("echo_start", Some(seq))?;
        let glyph = glyphs[(seq as usize) % glyphs.len()] as char;
        write!(out, "{glyph}")?;
        out.flush()?;
        if seq + 1 != iterations {
            thread::sleep(ECHO_TRAIN_INTERVAL);
        }
    }
    writeln!(out)?;
    out.flush()?;

    sleep_for_remaining(start, duration, ECHO_TRAIN_POST_IDLE);
    marker_writer.flush()?;
    Ok(())
}

fn run_steady_scroll(duration: Duration) -> Result<()> {
    let start = Instant::now();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut line = 0u64;
    while start.elapsed() < duration {
        writeln!(
            out,
            "scroll {line:06}  the quick brown fox jumps over the lazy dog 0123456789"
        )?;
        out.flush()?;
        line = line.saturating_add(1);
        thread::sleep(Duration::from_millis(6));
    }
    Ok(())
}

fn sleep_for_remaining(start: Instant, total_duration: Duration, requested_sleep: Duration) {
    let remaining = total_duration.saturating_sub(start.elapsed());
    thread::sleep(requested_sleep.min(remaining));
}

struct BenchmarkMarkerWriter {
    writer: io::BufWriter<fs::File>,
}

impl BenchmarkMarkerWriter {
    fn new_from_env() -> Result<Self> {
        let path = env::var(BENCHMARK_EVENTS_PATH_ENV)
            .context("missing TERMY_BENCHMARK_EVENTS_PATH for benchmark-driver")?;
        let path = PathBuf::from(path);
        let parent = path
            .parent()
            .context("benchmark events path is missing a parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let file = fs::File::create(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self {
            writer: io::BufWriter::new(file),
        })
    }

    fn record(&mut self, kind: &str, seq: Option<u64>) -> Result<()> {
        let marker = MarkerEvent {
            kind: kind.to_string(),
            seq,
            monotonic_ns: terminal_ui_monotonic_now_ns(),
        };
        serde_json::to_writer(&mut self.writer, &marker)
            .context("failed to serialize benchmark marker")?;
        self.writer
            .write_all(b"\n")
            .context("failed to write benchmark marker newline")?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .context("failed to flush benchmark marker")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MarkerEvent {
    kind: String,
    seq: Option<u64>,
    monotonic_ns: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FrameEvent {
    monotonic_ns: u64,
    elapsed_ms: u64,
    total_frames: u64,
    terminal_redraws: u64,
    view_wake_signals: u64,
    runtime_wakeups: u64,
}

fn run_alt_screen_anim(duration: Duration) -> Result<()> {
    let start = Instant::now();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write!(out, "\x1b[?1049h\x1b[?25l")?;
    out.flush()?;

    let rows = 24usize;
    let cols = 80usize;
    let mut frame = 0usize;
    while start.elapsed() < duration {
        write!(out, "\x1b[H")?;
        for row in 0..rows {
            let band = (row + frame) % cols;
            let mut line = String::with_capacity(cols);
            for col in 0..cols {
                let ch = if col == band || col == (band + 1) % cols {
                    '#'
                } else if (col + row + frame) % 7 == 0 {
                    '.'
                } else {
                    ' '
                };
                line.push(ch);
            }
            writeln!(out, "{line}")?;
        }
        write!(
            out,
            "\x1b[2;2Hframe {:05} elapsed {:.2}s\x1b[{};{}H",
            frame,
            start.elapsed().as_secs_f32(),
            (frame % rows) + 1,
            ((frame * 3) % cols) + 1
        )?;
        out.flush()?;
        frame = frame.saturating_add(1);
        thread::sleep(Duration::from_millis(16));
    }

    write!(out, "\x1b[?25h\x1b[?1049l")?;
    out.flush()?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AppSummary {
    build_label: Option<String>,
    git_sha: Option<String>,
    scenario: String,
    duration_ms: u64,
    sample_count: u64,
    total_frames: u64,
    fps_avg: f32,
    frame_p50_ms: f32,
    frame_p95_ms: f32,
    frame_p99_ms: f32,
    cpu_avg_percent: f32,
    cpu_max_percent: f32,
    memory_max_bytes: u64,
    runtime_wakeups: u64,
    view_wake_signals: u64,
    terminal_event_drain_passes: u64,
    terminal_redraws: u64,
    alt_screen_fallback_redraws: u64,
    grid_paint_count: u64,
    shape_line_calls: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EnergySummary {
    trace_template: String,
    cpu_total_ns: Option<u64>,
    cpu_percent: Option<f32>,
    idle_wakeups: Option<u64>,
    memory_bytes: Option<u64>,
    disk_bytes_read: Option<u64>,
    disk_bytes_written: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct RunResult {
    build_label: String,
    git_sha: String,
    scenario: String,
    app_summary: AppSummary,
    energy_summary: EnergySummary,
    micro_latency: MicroLatencySummary,
}

#[derive(Clone, Debug, Default, Serialize)]
struct MicroLatencySummary {
    idle_burst: Option<IdleBurstLatencySummary>,
    echo_train: Option<EchoTrainLatencySummary>,
}

#[derive(Clone, Debug, Serialize)]
struct IdleBurstLatencySummary {
    first_frame_after_burst_ms: Option<f32>,
    last_frame_after_burst_ms: Option<f32>,
    frames_until_settle: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct EchoTrainLatencySummary {
    echo_first_frame_ms_p50: Option<f32>,
    echo_first_frame_ms_p95: Option<f32>,
    echo_first_frame_ms_p99: Option<f32>,
    echo_first_frame_ms_max: Option<f32>,
    echo_missed_count: u64,
    echo_sample_count: u64,
}

fn run_single_benchmark(
    build: &BuildSpec,
    scenario: Scenario,
    duration_secs: u64,
    output_root: &Path,
) -> Result<RunResult> {
    let raw_dir = output_root
        .join("raw")
        .join(build.label)
        .join(scenario.as_str());
    let energy_dir = output_root
        .join("energy")
        .join(build.label)
        .join(scenario.as_str());
    let config_root = raw_dir.join("config");
    let metrics_dir = raw_dir.join("app");
    let driver_dir = raw_dir.join("driver");
    if raw_dir.exists() {
        fs::remove_dir_all(&raw_dir)
            .with_context(|| format!("failed to clear {}", raw_dir.display()))?;
    }
    if energy_dir.exists() {
        fs::remove_dir_all(&energy_dir)
            .with_context(|| format!("failed to clear {}", energy_dir.display()))?;
    }
    fs::create_dir_all(config_root.join("termy"))
        .with_context(|| format!("failed to create {}", config_root.display()))?;
    fs::create_dir_all(&metrics_dir)
        .with_context(|| format!("failed to create {}", metrics_dir.display()))?;
    fs::create_dir_all(&driver_dir)
        .with_context(|| format!("failed to create {}", driver_dir.display()))?;
    fs::create_dir_all(&energy_dir)
        .with_context(|| format!("failed to create {}", energy_dir.display()))?;

    let config_path = config_root.join("termy/config.txt");
    fs::write(&config_path, benchmark_config_contents())
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    let trace_path = energy_dir.join("activity-monitor.trace");
    let markers_path = driver_dir.join("markers.ndjson");
    let command = benchmark_driver_command(build, scenario, duration_secs);
    let time_limit_secs = duration_secs.saturating_add(TRACE_PADDING_SECS);
    let mut activity_command = activity_monitor_command(
        build,
        &trace_path,
        &config_root,
        &metrics_dir,
        &markers_path,
        scenario,
        &command,
        time_limit_secs,
    );
    run_command(
        &mut activity_command,
        format!(
            "xctrace benchmark run for {} {}",
            build.label,
            scenario.as_str()
        ),
    )?;

    let summary_path = metrics_dir.join("summary.json");
    let summary: AppSummary = read_json(&summary_path)?;
    let frames_path = metrics_dir.join("frames.ndjson");
    let micro_latency = summarize_micro_latency(scenario, &markers_path, &frames_path)?;

    let toc_path = energy_dir.join("toc.xml");
    let live_path = energy_dir.join("activity-monitor-process-live.xml");
    let ledger_path = energy_dir.join("activity-monitor-process-ledger.xml");
    export_xctrace_table(&trace_path, None, &toc_path)?;
    export_xctrace_table(
        &trace_path,
        Some("/trace-toc/run[@number=\"1\"]/data/table[@schema=\"activity-monitor-process-live\"]"),
        &live_path,
    )?;
    export_xctrace_table(
        &trace_path,
        Some(
            "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"activity-monitor-process-ledger\"]",
        ),
        &ledger_path,
    )?;

    let energy_summary = parse_activity_monitor_summary(&live_path, &ledger_path)?;
    let energy_json_path = energy_dir.join("energy.json");
    write_json(&energy_json_path, &energy_summary)?;

    Ok(RunResult {
        build_label: build.label.to_string(),
        git_sha: build.git_sha.clone(),
        scenario: scenario.as_str().to_string(),
        app_summary: summary,
        energy_summary,
        micro_latency,
    })
}

fn benchmark_config_contents() -> &'static str {
    "tmux_enabled = false\nbackground_blur = false\nbackground_opacity = 1.0\ncursor_blink = false\nwindow_width = 1280\nwindow_height = 820\nshow_debug_overlay = false\n"
}

fn benchmark_driver_command(build: &BuildSpec, scenario: Scenario, duration_secs: u64) -> String {
    format!(
        "{} benchmark-driver --scenario {} --duration-secs {}",
        shell_escape_path(&build.xtask_binary()),
        scenario.as_str(),
        duration_secs
    )
}

fn activity_monitor_command(
    build: &BuildSpec,
    trace_path: &Path,
    config_root: &Path,
    metrics_dir: &Path,
    markers_path: &Path,
    scenario: Scenario,
    benchmark_command: &str,
    time_limit_secs: u64,
) -> Command {
    let mut command = Command::new("xctrace");
    command
        .arg("record")
        .arg("--template")
        .arg("Activity Monitor")
        .arg("--time-limit")
        .arg(format!("{time_limit_secs}s"))
        .arg("--output")
        .arg(trace_path)
        .arg("--env")
        .arg(format!("XDG_CONFIG_HOME={}", config_root.display()))
        .arg("--env")
        .arg(format!("TERMY_BENCHMARK_COMMAND={benchmark_command}"))
        .arg("--env")
        .arg(format!("TERMY_BENCHMARK_SCENARIO={}", scenario.as_str()))
        .arg("--env")
        .arg(format!(
            "TERMY_BENCHMARK_METRICS_PATH={}",
            metrics_dir.display()
        ))
        .arg("--env")
        .arg(format!(
            "{BENCHMARK_EVENTS_PATH_ENV}={}",
            markers_path.display()
        ))
        .arg("--env")
        .arg("TERMY_BENCHMARK_EXIT_ON_COMPLETE=1")
        .arg("--env")
        .arg(format!("TERMY_BENCHMARK_BUILD_LABEL={}", build.label))
        .arg("--env")
        .arg(format!("TERMY_BENCHMARK_GIT_SHA={}", build.git_sha))
        .arg("--launch")
        .arg("--")
        .arg(build.termy_binary());
    command
}

fn export_xctrace_table(trace_path: &Path, xpath: Option<&str>, output_path: &Path) -> Result<()> {
    let mut command = Command::new("xctrace");
    command.arg("export").arg("--input").arg(trace_path);
    if let Some(xpath) = xpath {
        command.arg("--xpath").arg(xpath);
    } else {
        command.arg("--toc");
    }
    command.arg("--output").arg(output_path);
    run_command(
        &mut command,
        format!("xctrace export {}", output_path.display()),
    )
}

fn parse_activity_monitor_summary(live_path: &Path, ledger_path: &Path) -> Result<EnergySummary> {
    let live_xml = fs::read_to_string(live_path)
        .with_context(|| format!("failed to read {}", live_path.display()))?;
    let ledger_xml = fs::read_to_string(ledger_path)
        .with_context(|| format!("failed to read {}", ledger_path.display()))?;

    let live_row = parse_single_row_table(&live_xml)?;
    let ledger_row = parse_single_row_table(&ledger_xml)?;

    Ok(EnergySummary {
        trace_template: "Activity Monitor".to_string(),
        cpu_total_ns: ledger_row
            .get("cpu-total")
            .and_then(|value| value.parse::<u64>().ok()),
        cpu_percent: live_row
            .get("cpu-percent")
            .and_then(|value| value.parse::<f32>().ok()),
        idle_wakeups: ledger_row
            .get("idle-wakeups")
            .and_then(|value| value.parse::<u64>().ok()),
        memory_bytes: live_row
            .get("memory-physical-footprint")
            .and_then(|value| value.parse::<u64>().ok()),
        disk_bytes_read: ledger_row
            .get("disk-bytes-read")
            .and_then(|value| value.parse::<u64>().ok()),
        disk_bytes_written: ledger_row
            .get("disk-bytes-written")
            .and_then(|value| value.parse::<u64>().ok()),
    })
}

fn summarize_micro_latency(
    scenario: Scenario,
    markers_path: &Path,
    frames_path: &Path,
) -> Result<MicroLatencySummary> {
    match scenario {
        Scenario::IdleBurst => {
            let markers: Vec<MarkerEvent> = read_ndjson(markers_path)?;
            let frames: Vec<FrameEvent> = read_ndjson(frames_path)?;
            Ok(MicroLatencySummary {
                idle_burst: Some(summarize_idle_burst_latency(&markers, &frames)?),
                echo_train: None,
            })
        }
        Scenario::EchoTrain => {
            let markers: Vec<MarkerEvent> = read_ndjson(markers_path)?;
            let frames: Vec<FrameEvent> = read_ndjson(frames_path)?;
            Ok(MicroLatencySummary {
                idle_burst: None,
                echo_train: Some(summarize_echo_train_latency(&markers, &frames)?),
            })
        }
        Scenario::SteadyScroll | Scenario::AltScreenAnim => Ok(MicroLatencySummary::default()),
    }
}

fn summarize_idle_burst_latency(
    markers: &[MarkerEvent],
    frames: &[FrameEvent],
) -> Result<IdleBurstLatencySummary> {
    let burst_start = markers
        .iter()
        .find(|marker| marker.kind == "burst_start")
        .context("missing burst_start marker")?
        .monotonic_ns;
    let burst_end = markers
        .iter()
        .find(|marker| marker.kind == "burst_end")
        .context("missing burst_end marker")?
        .monotonic_ns;
    let first_frame = frames
        .iter()
        .find(|frame| frame.monotonic_ns >= burst_start);
    let last_frame = frames
        .iter()
        .rev()
        .find(|frame| frame.monotonic_ns >= burst_end);
    let frames_before_burst = frames
        .iter()
        .rev()
        .find(|frame| frame.monotonic_ns < burst_start)
        .map(|frame| frame.total_frames)
        .unwrap_or(0);

    Ok(IdleBurstLatencySummary {
        first_frame_after_burst_ms: first_frame
            .map(|frame| nanos_to_millis(frame.monotonic_ns.saturating_sub(burst_start))),
        last_frame_after_burst_ms: last_frame
            .map(|frame| nanos_to_millis(frame.monotonic_ns.saturating_sub(burst_end))),
        frames_until_settle: last_frame
            .map(|frame| frame.total_frames.saturating_sub(frames_before_burst)),
    })
}

fn summarize_echo_train_latency(
    markers: &[MarkerEvent],
    frames: &[FrameEvent],
) -> Result<EchoTrainLatencySummary> {
    let echo_markers: Vec<&MarkerEvent> = markers
        .iter()
        .filter(|marker| marker.kind == "echo_start")
        .collect();
    if echo_markers.is_empty() {
        bail!("missing echo_start markers");
    }

    let mut latencies_ns = Vec::with_capacity(echo_markers.len());
    let mut missed_count = 0u64;
    for marker in echo_markers {
        if let Some(frame) = frames
            .iter()
            .find(|frame| frame.monotonic_ns >= marker.monotonic_ns)
        {
            latencies_ns.push(frame.monotonic_ns.saturating_sub(marker.monotonic_ns));
        } else {
            missed_count = missed_count.saturating_add(1);
        }
    }

    let mut sorted = latencies_ns;
    sorted.sort_unstable();
    Ok(EchoTrainLatencySummary {
        echo_first_frame_ms_p50: (!sorted.is_empty())
            .then(|| percentile_nanos_to_millis(&sorted, 50, 100)),
        echo_first_frame_ms_p95: (!sorted.is_empty())
            .then(|| percentile_nanos_to_millis(&sorted, 95, 100)),
        echo_first_frame_ms_p99: (!sorted.is_empty())
            .then(|| percentile_nanos_to_millis(&sorted, 99, 100)),
        echo_first_frame_ms_max: sorted.last().copied().map(nanos_to_millis),
        echo_missed_count: missed_count,
        echo_sample_count: sorted.len() as u64,
    })
}

fn parse_single_row_table(xml: &str) -> Result<std::collections::HashMap<String, String>> {
    let doc = Document::parse(xml).context("failed to parse xctrace xml")?;
    let node = doc
        .descendants()
        .find(|node| node.has_tag_name("node"))
        .context("missing trace-query-result node")?;
    let schema = node
        .children()
        .find(|child| child.has_tag_name("schema"))
        .context("missing schema node")?;
    let mnemonics = schema
        .children()
        .filter(|child| child.has_tag_name("col"))
        .map(schema_mnemonic)
        .collect::<Result<Vec<_>>>()?;
    let row = node
        .children()
        .find(|child| child.has_tag_name("row"))
        .context("missing row node")?;

    let mut values = std::collections::HashMap::new();
    for (mnemonic, cell) in mnemonics
        .into_iter()
        .zip(row.children().filter(Node::is_element))
    {
        if cell.has_tag_name("sentinel") {
            continue;
        }
        if let Some(value) = node_value(cell) {
            values.insert(mnemonic, value);
        }
    }
    Ok(values)
}

fn schema_mnemonic(node: Node<'_, '_>) -> Result<String> {
    node.children()
        .find(|child| child.has_tag_name("mnemonic"))
        .and_then(|mnemonic| mnemonic.text())
        .map(ToOwned::to_owned)
        .context("missing schema mnemonic")
}

fn node_value(node: Node<'_, '_>) -> Option<String> {
    node.text()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn shell_escape_path(path: &Path) -> String {
    shell_escape(path.as_os_str())
}

fn shell_escape(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn run_command(command: &mut Command, description: String) -> Result<()> {
    let status = command
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to start {description}"))?;
    if !status.success() {
        bail!("{description} failed with status {status}");
    }
    Ok(())
}

fn read_ndjson<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("failed to parse ndjson row"))
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn nanos_to_millis(nanos: u64) -> f32 {
    nanos as f32 / 1_000_000.0
}

fn percentile_nanos_to_millis(samples: &[u64], numerator: usize, denominator: usize) -> f32 {
    let Some(last_index) = samples.len().checked_sub(1) else {
        return 0.0;
    };
    let index =
        (last_index.saturating_mul(numerator) + denominator.saturating_sub(1)) / denominator;
    nanos_to_millis(samples[index])
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let contents = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
}

#[derive(Clone, Debug, Serialize)]
struct ComparisonSummary {
    baseline_git_sha: String,
    candidate_git_sha: String,
    scenarios: Vec<ScenarioComparison>,
}

impl ComparisonSummary {
    fn from_runs(
        baseline: &BuildSpec,
        candidate: &BuildSpec,
        runs: Vec<RunResult>,
    ) -> Result<Self> {
        let mut scenarios = Vec::new();
        for scenario in Scenario::all() {
            let baseline_run = runs
                .iter()
                .find(|run| run.build_label == baseline.label && run.scenario == scenario.as_str())
                .cloned()
                .with_context(|| format!("missing baseline run for {}", scenario.as_str()))?;
            let candidate_run = runs
                .iter()
                .find(|run| run.build_label == candidate.label && run.scenario == scenario.as_str())
                .cloned()
                .with_context(|| format!("missing candidate run for {}", scenario.as_str()))?;
            scenarios.push(ScenarioComparison::new(
                scenario.as_str().to_string(),
                baseline_run,
                candidate_run,
            ));
        }

        Ok(Self {
            baseline_git_sha: baseline.git_sha.clone(),
            candidate_git_sha: candidate.git_sha.clone(),
            scenarios,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct ScenarioComparison {
    scenario: String,
    baseline: RunResult,
    candidate: RunResult,
    deltas: ScenarioDeltas,
}

impl ScenarioComparison {
    fn new(scenario: String, baseline: RunResult, candidate: RunResult) -> Self {
        let deltas = ScenarioDeltas {
            frame_p95_ms: candidate.app_summary.frame_p95_ms - baseline.app_summary.frame_p95_ms,
            frame_p99_ms: candidate.app_summary.frame_p99_ms - baseline.app_summary.frame_p99_ms,
            cpu_avg_percent: candidate.app_summary.cpu_avg_percent
                - baseline.app_summary.cpu_avg_percent,
            idle_wakeups: option_delta(
                candidate.energy_summary.idle_wakeups,
                baseline.energy_summary.idle_wakeups,
            ),
            idle_burst_first_frame_ms: option_f32_delta(
                candidate
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.first_frame_after_burst_ms),
                baseline
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.first_frame_after_burst_ms),
            ),
            idle_burst_last_frame_ms: option_f32_delta(
                candidate
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.last_frame_after_burst_ms),
                baseline
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.last_frame_after_burst_ms),
            ),
            idle_burst_frames_until_settle: option_i64_delta(
                candidate
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.frames_until_settle),
                baseline
                    .micro_latency
                    .idle_burst
                    .as_ref()
                    .and_then(|summary| summary.frames_until_settle),
            ),
            echo_first_frame_ms_p95: option_f32_delta(
                candidate
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .and_then(|summary| summary.echo_first_frame_ms_p95),
                baseline
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .and_then(|summary| summary.echo_first_frame_ms_p95),
            ),
            echo_first_frame_ms_max: option_f32_delta(
                candidate
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .and_then(|summary| summary.echo_first_frame_ms_max),
                baseline
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .and_then(|summary| summary.echo_first_frame_ms_max),
            ),
            echo_missed_count: option_i64_delta(
                candidate
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .map(|summary| summary.echo_missed_count),
                baseline
                    .micro_latency
                    .echo_train
                    .as_ref()
                    .map(|summary| summary.echo_missed_count),
            ),
        };
        Self {
            scenario,
            baseline,
            candidate,
            deltas,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ScenarioDeltas {
    frame_p95_ms: f32,
    frame_p99_ms: f32,
    cpu_avg_percent: f32,
    idle_wakeups: Option<i64>,
    idle_burst_first_frame_ms: Option<f32>,
    idle_burst_last_frame_ms: Option<f32>,
    idle_burst_frames_until_settle: Option<i64>,
    echo_first_frame_ms_p95: Option<f32>,
    echo_first_frame_ms_max: Option<f32>,
    echo_missed_count: Option<i64>,
}

fn option_delta(candidate: Option<u64>, baseline: Option<u64>) -> Option<i64> {
    match (candidate, baseline) {
        (Some(candidate), Some(baseline)) => Some(candidate as i64 - baseline as i64),
        _ => None,
    }
}

fn option_i64_delta(candidate: Option<u64>, baseline: Option<u64>) -> Option<i64> {
    match (candidate, baseline) {
        (Some(candidate), Some(baseline)) => Some(candidate as i64 - baseline as i64),
        _ => None,
    }
}

fn option_f32_delta(candidate: Option<f32>, baseline: Option<f32>) -> Option<f32> {
    match (candidate, baseline) {
        (Some(candidate), Some(baseline)) => Some(candidate - baseline),
        _ => None,
    }
}

fn write_report_artifacts(output_root: &Path, summary: &ComparisonSummary) -> Result<()> {
    write_json(&output_root.join("summary.json"), summary)?;
    fs::write(output_root.join("report.md"), render_report(summary)).with_context(|| {
        format!(
            "failed to write {}",
            output_root.join("report.md").display()
        )
    })
}

fn render_report(summary: &ComparisonSummary) -> String {
    let mut report = String::new();
    report.push_str("# Termy Render Benchmark Report\n\n");
    report.push_str(&format!(
        "Baseline `{}` vs candidate `{}`.\n\n",
        summary.baseline_git_sha, summary.candidate_git_sha
    ));

    for scenario in &summary.scenarios {
        report.push_str(&format!("## {}\n\n", scenario.scenario));
        report.push_str("| Metric | Baseline | Candidate | Delta |\n");
        report.push_str("| --- | ---: | ---: | ---: |\n");
        report.push_str(&format!(
            "| Frame p50 ms | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.frame_p50_ms,
            scenario.candidate.app_summary.frame_p50_ms,
            scenario.candidate.app_summary.frame_p50_ms
                - scenario.baseline.app_summary.frame_p50_ms
        ));
        report.push_str(&format!(
            "| Frame p95 ms | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.frame_p95_ms,
            scenario.candidate.app_summary.frame_p95_ms,
            scenario.deltas.frame_p95_ms
        ));
        report.push_str(&format!(
            "| Frame p99 ms | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.frame_p99_ms,
            scenario.candidate.app_summary.frame_p99_ms,
            scenario.deltas.frame_p99_ms
        ));
        report.push_str(&format!(
            "| FPS avg | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.fps_avg,
            scenario.candidate.app_summary.fps_avg,
            scenario.candidate.app_summary.fps_avg - scenario.baseline.app_summary.fps_avg
        ));
        report.push_str(&format!(
            "| CPU avg % | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.cpu_avg_percent,
            scenario.candidate.app_summary.cpu_avg_percent,
            scenario.deltas.cpu_avg_percent
        ));
        report.push_str(&format!(
            "| CPU max % | {:.2} | {:.2} | {:.2} |\n",
            scenario.baseline.app_summary.cpu_max_percent,
            scenario.candidate.app_summary.cpu_max_percent,
            scenario.candidate.app_summary.cpu_max_percent
                - scenario.baseline.app_summary.cpu_max_percent
        ));
        report.push_str(&format!(
            "| Runtime wakeups | {} | {} | {} |\n",
            scenario.baseline.app_summary.runtime_wakeups,
            scenario.candidate.app_summary.runtime_wakeups,
            scenario.candidate.app_summary.runtime_wakeups as i64
                - scenario.baseline.app_summary.runtime_wakeups as i64
        ));
        report.push_str(&format!(
            "| View wake signals | {} | {} | {} |\n",
            scenario.baseline.app_summary.view_wake_signals,
            scenario.candidate.app_summary.view_wake_signals,
            scenario.candidate.app_summary.view_wake_signals as i64
                - scenario.baseline.app_summary.view_wake_signals as i64
        ));
        report.push_str(&format!(
            "| Drain passes | {} | {} | {} |\n",
            scenario.baseline.app_summary.terminal_event_drain_passes,
            scenario.candidate.app_summary.terminal_event_drain_passes,
            scenario.candidate.app_summary.terminal_event_drain_passes as i64
                - scenario.baseline.app_summary.terminal_event_drain_passes as i64
        ));
        report.push_str(&format!(
            "| Redraws | {} | {} | {} |\n",
            scenario.baseline.app_summary.terminal_redraws,
            scenario.candidate.app_summary.terminal_redraws,
            scenario.candidate.app_summary.terminal_redraws as i64
                - scenario.baseline.app_summary.terminal_redraws as i64
        ));
        report.push_str(&format!(
            "| Alt-screen fallback redraws | {} | {} | {} |\n",
            scenario.baseline.app_summary.alt_screen_fallback_redraws,
            scenario.candidate.app_summary.alt_screen_fallback_redraws,
            scenario.candidate.app_summary.alt_screen_fallback_redraws as i64
                - scenario.baseline.app_summary.alt_screen_fallback_redraws as i64
        ));
        report.push_str(&format!(
            "| Idle wakeups | {} | {} | {} |\n\n",
            format_option_u64(scenario.baseline.energy_summary.idle_wakeups),
            format_option_u64(scenario.candidate.energy_summary.idle_wakeups),
            format_option_i64(scenario.deltas.idle_wakeups),
        ));

        if let (Some(baseline), Some(candidate)) = (
            scenario.baseline.micro_latency.idle_burst.as_ref(),
            scenario.candidate.micro_latency.idle_burst.as_ref(),
        ) {
            report.push_str("| Idle-burst metric | Baseline | Candidate | Delta |\n");
            report.push_str("| --- | ---: | ---: | ---: |\n");
            report.push_str(&format!(
                "| First frame after burst ms | {} | {} | {} |\n",
                format_option_f32(baseline.first_frame_after_burst_ms),
                format_option_f32(candidate.first_frame_after_burst_ms),
                format_option_f32(scenario.deltas.idle_burst_first_frame_ms),
            ));
            report.push_str(&format!(
                "| Last frame after burst ms | {} | {} | {} |\n",
                format_option_f32(baseline.last_frame_after_burst_ms),
                format_option_f32(candidate.last_frame_after_burst_ms),
                format_option_f32(scenario.deltas.idle_burst_last_frame_ms),
            ));
            report.push_str(&format!(
                "| Frames until settle | {} | {} | {} |\n\n",
                format_option_u64(baseline.frames_until_settle),
                format_option_u64(candidate.frames_until_settle),
                format_option_i64(scenario.deltas.idle_burst_frames_until_settle),
            ));
        }

        if let (Some(baseline), Some(candidate)) = (
            scenario.baseline.micro_latency.echo_train.as_ref(),
            scenario.candidate.micro_latency.echo_train.as_ref(),
        ) {
            report.push_str("| Echo-train metric | Baseline | Candidate | Delta |\n");
            report.push_str("| --- | ---: | ---: | ---: |\n");
            report.push_str(&format!(
                "| First frame p50 ms | {} | {} | {} |\n",
                format_option_f32(baseline.echo_first_frame_ms_p50),
                format_option_f32(candidate.echo_first_frame_ms_p50),
                format_option_f32(option_f32_delta(
                    candidate.echo_first_frame_ms_p50,
                    baseline.echo_first_frame_ms_p50
                )),
            ));
            report.push_str(&format!(
                "| First frame p95 ms | {} | {} | {} |\n",
                format_option_f32(baseline.echo_first_frame_ms_p95),
                format_option_f32(candidate.echo_first_frame_ms_p95),
                format_option_f32(scenario.deltas.echo_first_frame_ms_p95),
            ));
            report.push_str(&format!(
                "| First frame p99 ms | {} | {} | {} |\n",
                format_option_f32(baseline.echo_first_frame_ms_p99),
                format_option_f32(candidate.echo_first_frame_ms_p99),
                format_option_f32(option_f32_delta(
                    candidate.echo_first_frame_ms_p99,
                    baseline.echo_first_frame_ms_p99
                )),
            ));
            report.push_str(&format!(
                "| First frame max ms | {} | {} | {} |\n",
                format_option_f32(baseline.echo_first_frame_ms_max),
                format_option_f32(candidate.echo_first_frame_ms_max),
                format_option_f32(scenario.deltas.echo_first_frame_ms_max),
            ));
            report.push_str(&format!(
                "| Missed echoes | {} | {} | {} |\n",
                baseline.echo_missed_count,
                candidate.echo_missed_count,
                format_option_i64(scenario.deltas.echo_missed_count),
            ));
            report.push_str(&format!(
                "| Echo samples | {} | {} | {} |\n\n",
                baseline.echo_sample_count,
                candidate.echo_sample_count,
                candidate.echo_sample_count as i64 - baseline.echo_sample_count as i64,
            ));
        }

        report.push_str("Findings:\n");
        report.push_str(&format!(
            "- Candidate {} frame p95 by {:.2} ms.\n",
            if scenario.deltas.frame_p95_ms < 0.0 {
                "improves"
            } else {
                "regresses"
            },
            scenario.deltas.frame_p95_ms.abs()
        ));
        report.push_str(&format!(
            "- Candidate {} CPU avg by {:.2}%.\n\n",
            if scenario.deltas.cpu_avg_percent < 0.0 {
                "reduces"
            } else {
                "increases"
            },
            scenario.deltas.cpu_avg_percent.abs()
        ));
        if let Some(delta) = scenario.deltas.idle_burst_first_frame_ms {
            report.push_str(&format!(
                "- Candidate {} idle-burst first-frame latency by {:.2} ms.\n",
                if delta < 0.0 { "improves" } else { "regresses" },
                delta.abs()
            ));
        }
        if let Some(delta) = scenario.deltas.echo_first_frame_ms_p95 {
            report.push_str(&format!(
                "- Candidate {} echo-train first-frame p95 by {:.2} ms.\n",
                if delta < 0.0 { "improves" } else { "regresses" },
                delta.abs()
            ));
        }
        report.push('\n');
    }

    report
}

fn format_option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_option_i64(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_option_f32(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "n/a".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        FrameEvent, MarkerEvent, Scenario, parse_single_row_table, render_report,
        summarize_echo_train_latency, summarize_idle_burst_latency,
    };

    #[test]
    fn parses_scenario_names() {
        assert_eq!(Scenario::parse("idle-burst").unwrap(), Scenario::IdleBurst);
        assert_eq!(Scenario::parse("echo-train").unwrap(), Scenario::EchoTrain);
        assert!(Scenario::parse("nope").is_err());
    }

    #[test]
    fn parses_single_row_xctrace_table() {
        let xml = r#"<?xml version="1.0"?>
<trace-query-result>
  <node>
    <schema name="example">
      <col><mnemonic>cpu-total</mnemonic></col>
      <col><mnemonic>idle-wakeups</mnemonic></col>
    </schema>
    <row>
      <duration-on-core>42</duration-on-core>
      <event-count>7</event-count>
    </row>
  </node>
</trace-query-result>"#;
        let parsed = parse_single_row_table(xml).unwrap();
        assert_eq!(parsed.get("cpu-total").unwrap(), "42");
        assert_eq!(parsed.get("idle-wakeups").unwrap(), "7");
    }

    #[test]
    fn summarizes_idle_burst_latency() {
        let markers = vec![
            MarkerEvent {
                kind: "burst_start".to_string(),
                seq: None,
                monotonic_ns: 100,
            },
            MarkerEvent {
                kind: "burst_end".to_string(),
                seq: None,
                monotonic_ns: 150,
            },
        ];
        let frames = vec![
            FrameEvent {
                monotonic_ns: 90,
                elapsed_ms: 0,
                total_frames: 1,
                terminal_redraws: 0,
                view_wake_signals: 0,
                runtime_wakeups: 0,
            },
            FrameEvent {
                monotonic_ns: 130,
                elapsed_ms: 1,
                total_frames: 2,
                terminal_redraws: 1,
                view_wake_signals: 1,
                runtime_wakeups: 1,
            },
            FrameEvent {
                monotonic_ns: 180,
                elapsed_ms: 2,
                total_frames: 4,
                terminal_redraws: 2,
                view_wake_signals: 2,
                runtime_wakeups: 2,
            },
        ];

        let summary = summarize_idle_burst_latency(&markers, &frames).unwrap();
        assert!(
            (summary.first_frame_after_burst_ms.unwrap() - 0.00003).abs() < f32::EPSILON,
            "unexpected first frame latency: {:?}",
            summary.first_frame_after_burst_ms
        );
        assert!(
            (summary.last_frame_after_burst_ms.unwrap() - 0.00003).abs() < f32::EPSILON,
            "unexpected last frame latency: {:?}",
            summary.last_frame_after_burst_ms
        );
        assert_eq!(summary.frames_until_settle, Some(3));
    }

    #[test]
    fn summarizes_echo_train_latency() {
        let markers = vec![
            MarkerEvent {
                kind: "echo_start".to_string(),
                seq: Some(0),
                monotonic_ns: 100,
            },
            MarkerEvent {
                kind: "echo_start".to_string(),
                seq: Some(1),
                monotonic_ns: 200,
            },
        ];
        let frames = vec![
            FrameEvent {
                monotonic_ns: 120,
                elapsed_ms: 1,
                total_frames: 1,
                terminal_redraws: 1,
                view_wake_signals: 1,
                runtime_wakeups: 1,
            },
            FrameEvent {
                monotonic_ns: 260,
                elapsed_ms: 2,
                total_frames: 2,
                terminal_redraws: 2,
                view_wake_signals: 2,
                runtime_wakeups: 2,
            },
        ];

        let summary = summarize_echo_train_latency(&markers, &frames).unwrap();
        assert_eq!(summary.echo_missed_count, 0);
        assert_eq!(summary.echo_sample_count, 2);
        assert!((summary.echo_first_frame_ms_max.unwrap() - 0.00006).abs() < f32::EPSILON);
    }

    #[test]
    fn render_report_mentions_micro_latency_tables() {
        let report = render_report(&super::ComparisonSummary {
            baseline_git_sha: "abc".to_string(),
            candidate_git_sha: "def".to_string(),
            scenarios: vec![super::ScenarioComparison {
                scenario: "idle-burst".to_string(),
                baseline: super::RunResult {
                    build_label: "baseline".to_string(),
                    git_sha: "abc".to_string(),
                    scenario: "idle-burst".to_string(),
                    app_summary: super::AppSummary {
                        build_label: Some("baseline".to_string()),
                        git_sha: Some("abc".to_string()),
                        scenario: "idle-burst".to_string(),
                        duration_ms: 3000,
                        sample_count: 2,
                        total_frames: 4,
                        fps_avg: 1.0,
                        frame_p50_ms: 12.0,
                        frame_p95_ms: 20.0,
                        frame_p99_ms: 25.0,
                        cpu_avg_percent: 3.0,
                        cpu_max_percent: 5.0,
                        memory_max_bytes: 1,
                        runtime_wakeups: 1,
                        view_wake_signals: 1,
                        terminal_event_drain_passes: 1,
                        terminal_redraws: 1,
                        alt_screen_fallback_redraws: 0,
                        grid_paint_count: 1,
                        shape_line_calls: 1,
                    },
                    energy_summary: super::EnergySummary {
                        trace_template: "Activity Monitor".to_string(),
                        cpu_total_ns: None,
                        cpu_percent: None,
                        idle_wakeups: Some(10),
                        memory_bytes: None,
                        disk_bytes_read: None,
                        disk_bytes_written: None,
                    },
                    micro_latency: super::MicroLatencySummary {
                        idle_burst: Some(super::IdleBurstLatencySummary {
                            first_frame_after_burst_ms: Some(4.0),
                            last_frame_after_burst_ms: Some(12.0),
                            frames_until_settle: Some(2),
                        }),
                        echo_train: None,
                    },
                },
                candidate: super::RunResult {
                    build_label: "candidate".to_string(),
                    git_sha: "def".to_string(),
                    scenario: "idle-burst".to_string(),
                    app_summary: super::AppSummary {
                        build_label: Some("candidate".to_string()),
                        git_sha: Some("def".to_string()),
                        scenario: "idle-burst".to_string(),
                        duration_ms: 3000,
                        sample_count: 2,
                        total_frames: 5,
                        fps_avg: 2.0,
                        frame_p50_ms: 8.0,
                        frame_p95_ms: 10.0,
                        frame_p99_ms: 12.0,
                        cpu_avg_percent: 4.0,
                        cpu_max_percent: 6.0,
                        memory_max_bytes: 1,
                        runtime_wakeups: 2,
                        view_wake_signals: 2,
                        terminal_event_drain_passes: 2,
                        terminal_redraws: 2,
                        alt_screen_fallback_redraws: 0,
                        grid_paint_count: 2,
                        shape_line_calls: 2,
                    },
                    energy_summary: super::EnergySummary {
                        trace_template: "Activity Monitor".to_string(),
                        cpu_total_ns: None,
                        cpu_percent: None,
                        idle_wakeups: Some(11),
                        memory_bytes: None,
                        disk_bytes_read: None,
                        disk_bytes_written: None,
                    },
                    micro_latency: super::MicroLatencySummary {
                        idle_burst: Some(super::IdleBurstLatencySummary {
                            first_frame_after_burst_ms: Some(2.0),
                            last_frame_after_burst_ms: Some(8.0),
                            frames_until_settle: Some(1),
                        }),
                        echo_train: None,
                    },
                },
                deltas: super::ScenarioDeltas {
                    frame_p95_ms: -10.0,
                    frame_p99_ms: -13.0,
                    cpu_avg_percent: 1.0,
                    idle_wakeups: Some(1),
                    idle_burst_first_frame_ms: Some(-2.0),
                    idle_burst_last_frame_ms: Some(-4.0),
                    idle_burst_frames_until_settle: Some(-1),
                    echo_first_frame_ms_p95: None,
                    echo_first_frame_ms_max: None,
                    echo_missed_count: None,
                },
            }],
        });
        assert!(report.contains("Idle-burst metric"));
        assert!(report.contains("First frame after burst ms"));
    }
}
