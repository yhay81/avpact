# AVPact performance baseline

This directory defines the reproducible, observation-only baseline used to
calibrate AVPact's v1.0 performance and resource thresholds. Timing and memory
are not yet release thresholds.

## Workload

`run.sh` creates a 500 ms, 160×90, 10 fps MPEG-4/AAC file from FFmpeg `lavfi`
`testsrc2` and `sine` sources. It then measures fresh CLI processes for:

- the compact contract catalog and complete plan JSON Schema;
- inspection of the synthetic input;
- compilation of a 100–400 ms clip plan;
- application, progress emission, verification, and atomic publication;
- an independent verification of the published output.

The harness records GNU `time` wall time and peak resident memory, stdout and
diagnostic sizes, input and output digests, the complete FFmpeg build identity,
plan shape, progress event count, receipt size and internal runtime, every
verification result, publication method, temporary-output cleanup, runner
identity, and the exact AVPact commit. Media generation and the release build
are excluded.

The inspect and plan samples deliberately include hashing and FFprobe/FFmpeg
preflight work. They are reproducible end-to-end upper bounds, not a claim
about isolated planner latency. The apply RSS is labeled as a CLI process-tree
observation; the current harness does not claim a separately sampled parent and
backend high-water mark.

## Run

The supported measurement environment is the `ubuntu-latest` GitHub-hosted
runner and distribution FFmpeg selected by
`.github/workflows/benchmark.yml`. Run it manually with the **Benchmark**
workflow, or on a compatible Linux machine:

```bash
benchmarks/run.sh benchmark-results.json
jq . benchmark-results.json
```

GNU `time`, GNU `stat`, `timeout`, FFmpeg, FFprobe, `jq`, Git, Cargo, and the
locked Rust dependency graph are required. Generated media, plans, receipts,
and intermediate output are temporary and are not uploaded.

The workflow retains raw JSON for 90 days. Pull requests gate semantic
correctness, bounds, publication cleanup, and verification—not observed timing
or memory. A single shared-runner sample is not a regression and does not
establish p95. Before enabling v1.0 performance gates, publish an isolated
planner measurement, separate parent/backend memory observations,
temporary-disk high-water measurement, runner and FFmpeg baseline window,
warm-up and sample policy, p95 calculation, and a noise-aware regression rule.
