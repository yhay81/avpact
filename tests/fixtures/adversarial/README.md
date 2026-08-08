# Adversarial safety corpus

`v0.1/corpus.json` publishes 60 deterministic labels for the five hostile or
racing conditions in the AVPact v1.0 correctness gate:

- 10 input identity changes after planning;
- 20 current-format receipt mutations while retaining the expected receipt ID;
- 10 output-verification failures;
- 10 unsafe input/output aliases; and
- 10 attempts to overwrite a destination created after planning.

The labels and expected stable error codes are generated independently of
AVPact by `v0.1/generate_corpus.py`. The Rust scorer passes every case through
the production `plan_recipe`, `apply_plan`, `verify_output`, or
`parse_receipt_document` path. Media fixtures are generated from FFmpeg `lavfi`
inside a temporary sandbox; the corpus metadata contains no third-party media.

`v0.1/metrics.json` records the canonical corpus digest, per-class detection
rate, destination changes, and leaked temporary paths. CI requires 100%
detection in every class, zero unintended destination changes, and zero
temporary-path leaks.

Reproduce the evidence on a host with `ffmpeg` and `ffprobe`:

```console
python3 tests/fixtures/adversarial/v0.1/generate_corpus.py --check
cargo test --test adversarial_corpus --locked
```

The corpus metadata and generator are licensed under the repository MIT
license.
