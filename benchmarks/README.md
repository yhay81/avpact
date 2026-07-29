# AVPact performance baseline

This directory defines and enforces AVPact's reproducible v1.0 performance and
resource thresholds on pull requests and in the weekly scheduled benchmark.

## Workload

`run.sh` creates a 500 ms, 160×90, 10 fps MPEG-4/AAC file from FFmpeg `lavfi`
`testsrc2` and `sine` sources. It then measures fresh CLI processes for:

- the compact contract catalog and complete plan JSON Schema;
- inspection of the synthetic input;
- compilation of a 100–400 ms clip plan;
- FFmpeg executed independently from the exact planned argument vector;
- application, progress emission, verification, and atomic publication;
- an independent verification of the published output.

The harness records GNU `time` wall time and peak resident memory, stdout and
diagnostic sizes, input and output digests, the complete FFmpeg build identity,
plan shape, progress event count, receipt size and internal runtime, every
verification result, publication method, temporary-output cleanup, runner
identity, and the exact AVPact commit. Media generation and the release build
are excluded.

The inspect and plan samples deliberately include hashing and FFprobe/FFmpeg
preflight work. The 250 ms planning threshold is applied to that stronger
end-to-end measurement, so the planner alone cannot exceed it. CLI GNU `time`
RSS includes any observed children and therefore upper-bounds the AVPact
parent. The independent FFmpeg invocation reports backend RSS separately and
records the completed temporary media size.

Each sample generates its own fixture and performs an untimed release build.
The workflow discards one warm-up and captures 20 samples on the same runner
image and FFmpeg build.

## Enforced thresholds

The versioned policy in `thresholds.json` enforces:

- contract generation and end-to-end planning below 250 ms p95;
- AVPact CLI process-tree RSS no greater than 256 MiB in every sample;
- isolated FFmpeg RSS no greater than 256 MiB in every sample;
- observed temporary media no greater than the 12 MiB fixture budget.

Twenty samples make nearest-rank p95 the second-slowest observation. Once
`baseline-ubuntu24.json` is present, each metric must also remain within the
stricter of its absolute limit and a versioned noise allowance.

## Run

The supported measurement environment is the `ubuntu-24.04` x86_64
GitHub-hosted runner and distribution FFmpeg selected by
`.github/workflows/benchmark.yml`. Run it manually with the **Benchmark**
workflow, or on a compatible Linux machine:

```bash
benchmarks/run.sh benchmark-results.json
jq . benchmark-results.json
```

Run evaluator tests with:

```bash
python3 -m unittest benchmarks/test_evaluate.py
```

GNU `time`, GNU `stat`, `timeout`, FFmpeg, FFprobe, `jq`, Git, Cargo, and the
locked Rust dependency graph are required. Generated media, plans, receipts,
and intermediate output are temporary and are not uploaded.

The workflow uploads all 20 raw samples and the aggregate evaluation for 90
days, including raw samples from a failed evaluation. The checked-in baseline
is refreshed only from a successful evaluation of the exact commit on the
fixed runner class. AVPact's runtime still enforces configured diagnostic,
progress, receipt, runtime, output, and temporary-file bounds independently of
the benchmark.
