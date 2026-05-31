#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MACOS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_PATH=""
MAX_STARTUP_MS=5000
MAX_RSS_MIB=512
MAX_IDLE_CPU_PERCENT=75

usage() {
  cat <<EOF
Usage: $0 --app PATH [options]

Launch a staged native Swift .app and gate basic startup, RSS, and idle CPU.

Options:
  --app PATH                 Staged TermyAlpha.app path
  --max-startup-ms N         Maximum time until process appears (default: 5000)
  --max-rss-mib N            Maximum resident memory after launch (default: 512)
  --max-idle-cpu-percent N   Maximum sampled CPU after launch (default: 75)
  --help                     Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      [[ $# -ge 2 ]] || { echo "Error: --app requires a value" >&2; exit 2; }
      APP_PATH="$2"
      shift 2
      ;;
    --max-startup-ms)
      [[ $# -ge 2 ]] || { echo "Error: --max-startup-ms requires a value" >&2; exit 2; }
      MAX_STARTUP_MS="$2"
      shift 2
      ;;
    --max-rss-mib)
      [[ $# -ge 2 ]] || { echo "Error: --max-rss-mib requires a value" >&2; exit 2; }
      MAX_RSS_MIB="$2"
      shift 2
      ;;
    --max-idle-cpu-percent)
      [[ $# -ge 2 ]] || { echo "Error: --max-idle-cpu-percent requires a value" >&2; exit 2; }
      MAX_IDLE_CPU_PERCENT="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ -n "$APP_PATH" ]] || { echo "Error: --app PATH is required" >&2; usage >&2; exit 2; }
[[ -d "$APP_PATH" ]] || { echo "Error: app bundle not found: $APP_PATH" >&2; exit 1; }

APP_BINARY="$APP_PATH/Contents/MacOS/TermyAlpha"
[[ -x "$APP_BINARY" ]] || { echo "Error: app binary not found: $APP_BINARY" >&2; exit 1; }

now_ms() {
  perl -MTime::HiRes=time -e 'printf "%.0f\n", time * 1000'
}

cleanup() {
  if [[ -n "${PID:-}" ]]; then
    kill "$PID" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

pkill -f "$APP_BINARY" >/dev/null 2>&1 || true

echo "==> Launching $APP_PATH"
START_MS="$(now_ms)"
/usr/bin/open -n "$APP_PATH"

PID=""
while :; do
  PID="$(pgrep -f "$APP_BINARY" | head -n1 || true)"
  CURRENT_MS="$(now_ms)"
  ELAPSED_MS=$((CURRENT_MS - START_MS))
  if [[ -n "$PID" ]]; then
    break
  fi
  if (( ELAPSED_MS > MAX_STARTUP_MS )); then
    echo "Error: app process did not appear within ${MAX_STARTUP_MS}ms" >&2
    exit 1
  fi
  sleep 0.05
done

sleep 1
RSS_KB="$(ps -o rss= -p "$PID" | awk '{ print $1 }')"
CPU_PERCENT="$(ps -o %cpu= -p "$PID" | awk '{ print $1 }')"
RSS_MIB="$(awk -v kb="$RSS_KB" 'BEGIN { printf "%.2f", kb / 1024 }')"

echo "Startup: ${ELAPSED_MS}ms"
echo "RSS: ${RSS_MIB} MiB"
echo "CPU sample: ${CPU_PERCENT}%"

if (( ELAPSED_MS > MAX_STARTUP_MS )); then
  echo "Error: startup ${ELAPSED_MS}ms exceeded ${MAX_STARTUP_MS}ms" >&2
  exit 1
fi

awk -v value="$RSS_MIB" -v max="$MAX_RSS_MIB" 'BEGIN { exit(value <= max ? 0 : 1) }' || {
  echo "Error: RSS ${RSS_MIB} MiB exceeded ${MAX_RSS_MIB} MiB" >&2
  exit 1
}

awk -v value="$CPU_PERCENT" -v max="$MAX_IDLE_CPU_PERCENT" 'BEGIN { exit(value <= max ? 0 : 1) }' || {
  echo "Error: CPU ${CPU_PERCENT}% exceeded ${MAX_IDLE_CPU_PERCENT}%" >&2
  exit 1
}

echo "Native launch gates passed"
