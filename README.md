# AVPact

Declarative, inspectable, and verifiable media transformations.

> Status: 0.1 implementation. The complete local-file operation set is
> implemented and covered by unit, CLI, and generated-media integration tests.

AVPact compiles a small set of media intents into a deterministic execution plan. The plan records streams, codecs, filters, expected outputs, resource limits, and verification checks before any transformation starts.

```bash
avpact schema --brief --format json
avpact capabilities --format json
avpact inspect input.mp4 --format json
avpact plan recipe.json --out plan.json --format json
avpact apply plan.json --progress ndjson --format json
avpact receipt show rcpt_0123... --format json
avpact verify output.mp4 --against plan.json --format json
```

## Why

FFmpeg is powerful but its option space is difficult to predict safely. Agents should choose a typed media intent, not synthesize a fragile shell command from memory.

## Product principles

- Declarative recipes are the source of truth.
- Inspect and plan before compute-heavy mutation.
- No shell interpolation.
- Deterministic encoder and filter selection is visible in the plan.
- Progress is emitted as bounded NDJSON events.
- Output properties are verified after encoding.
- The compiled FFmpeg argv remains inspectable.

## Supported operations

AVPact 0.1 supports inspection, clip, transcode, resize/fit/crop/pad/rotation,
audio extraction, measured loudness normalization, compatible concatenation,
thumbnails, contact sheets, and subtitle burn-in.

See [CONCEPT.md](CONCEPT.md) for the product model and [ROADMAP.md](ROADMAP.md)
for release gates. Typed recipes, constraints, and unsupported cases are
documented in [docs/OPERATIONS.md](docs/OPERATIONS.md).

## Install

Download a native archive from GitHub Releases, or install from a source
checkout:

```bash
cargo install --path . --locked
```

FFmpeg and FFprobe must be available on `PATH`. Because codec/filter support
varies by build, inspect it before automation:

```bash
avpact capabilities --format json
```

Generate completion scripts with `avpact completions bash` (also `zsh`,
`fish`, `power-shell`, and `elvish`).

## Build and try

Rust 1.85 or newer, FFmpeg, and FFprobe are required.

```bash
cargo build --release
cargo test --all-targets

./target/release/avpact inspect input.mp4 --format json
./target/release/avpact plan examples/clip.recipe.json \
  --out plan.json --format json
./target/release/avpact apply plan.json \
  --progress ndjson --format json
./target/release/avpact verify clip.mp4 \
  --against plan.json --format json
```

Recipe-relative input and output paths resolve from the recipe file's
directory. The example therefore expects `input.mp4` in the repository root and
publishes `clip.mp4` there.

By default, `apply` stores its receipt under `.avpact/receipts/` beside the
plan. Run `receipt show <receipt-id> --state-dir <plan-directory>/.avpact` from
another directory. Use `--receipt-out <path>` on `apply` when an explicit
standalone receipt path is preferable.

The execution contract applies to every operation:

- explicit default/first stream selection and warnings for dropped streams;
- inspectable FFmpeg argument arrays with capability checks;
- pinned FFmpeg build/library and FFprobe versions between plan and apply;
- verified codec, dimensions, square-pixel aspect, known frame rate, channels,
  and exact stream layout;
- no overwrite of an existing output;
- input identity revalidation before apply;
- hard output, temporary-disk, and runtime budgets;
- destination-adjacent temporary output, verification, then no-clobber
  same-filesystem publication;
- bounded NDJSON progress on stderr and final JSON on stdout;
- Ctrl-C cancellation with backend process-tree cleanup.

Use `avpact schema --document recipe --format json` (or `plan`,
`inspection`, `progress`, `verification`, `receipt`, `capability`, and `error`)
to retrieve the full JSON Schema for a document.

## Release integrity

CI tests Linux, macOS, Windows, and the declared Rust 1.85 MSRV. Tagged releases
contain native archives, documentation, shell completions, SHA-256 checksums,
and GitHub/Sigstore build provenance attestations. See
[RELEASING.md](RELEASING.md) and [SECURITY.md](SECURITY.md).

## License

MIT
