# Performance Before vs After

Generated from `/tmp/termy-benchmark-compare/summary.json` after the render-cache, metrics, and benchmark-harness changes.

Note: the full `benchmark-compare` run launched Termy in a macOS background/headless trace context, which only captured initial paint frames. Treat the frame metrics as limited signal; CPU, memory, wakeup, and benchmark-harness correctness are the useful comparisons here.

## Scenario Summary

| Scenario | Metric | Before | After | Delta |
| --- | --- | ---: | ---: | ---: |
| `idle-burst` | Avg CPU | 0.279% | 0.269% | -3.7% |
| `idle-burst` | Max memory | 90.44 MiB | 90.36 MiB | -0.08 MiB |
| `idle-burst` | Runtime wakeups | 2 | 2 | 0 |
| `idle-burst` | Frame p50 | 11.48 ms | 11.70 ms | +0.22 ms |
| `echo-train` | Avg CPU | 0.345% | 0.224% | -35.1% |
| `echo-train` | Max memory | 91.56 MiB | 91.38 MiB | -0.19 MiB |
| `echo-train` | Runtime wakeups | 22 | 22 | 0 |
| `echo-train` | Frame p50 | 16.30 ms | 11.47 ms | -4.83 ms |
| `steady-scroll` | Avg CPU | 0.800% | 0.756% | -5.4% |
| `steady-scroll` | Max memory | 96.86 MiB | 99.63 MiB | +2.77 MiB |
| `steady-scroll` | Runtime wakeups | 937 | 1094 | +157 |
| `steady-scroll` | Frame p50 | 13.01 ms | 12.36 ms | -0.65 ms |
| `alt-screen-anim` | Avg CPU | 0.663% | 0.588% | -11.4% |
| `alt-screen-anim` | Max memory | 94.59 MiB | 94.44 MiB | -0.16 MiB |
| `alt-screen-anim` | Runtime wakeups | 464 | 503 | +39 |
| `alt-screen-anim` | Frame p50 | 12.98 ms | 12.90 ms | -0.08 ms |

## Harness Correctness

| Check | Before | After |
| --- | ---: | ---: |
| `benchmark-compare` completion | Failed on missing `summary.json` / empty hitches export | Completed and wrote `/tmp/termy-benchmark-compare/report.md` |
| Direct `steady-scroll` drain passes | 0 | 548 |
| Direct `steady-scroll` redraws | 0 | 548 |
| Direct benchmark metrics files | Unreliable | Writes `summary.json`, `frames.ndjson`, and `timeline.ndjson` |

## Code Changes Behind The Numbers

| Area | Before | After |
| --- | --- | --- |
| Partial row-cache rebuilds | Allocated a full previous-row vector for partial dirty-row rebuilds | Stores only dirty previous rows |
| Render metrics | Hot paths always touched atomics | Counters stay dormant until benchmarks or explicit metrics sessions enable them |
| Benchmark shutdown | Relied on PTY exit to close the app | Uses explicit duration deadline fallback |
| Empty hitches export | Treated as fatal parser error | Treated as zero hitches |
| Benchmark terminal drain | Wakeups could be recorded without terminal event drains | Benchmark mode drains terminal events immediately |
