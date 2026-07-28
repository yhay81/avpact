# Fuzzing AVPact

AVPact continuously fuzzes its untrusted recipe boundary with AddressSanitizer.
The `recipe_document` target exercises the same bounded JSON parser and semantic
validator used before AVPact inspects media or invokes a backend.

Install a current nightly toolchain and the pinned local runner, then run:

```bash
cargo install cargo-fuzz --version 0.13.2 --locked
mkdir -p fuzz/corpus/recipe_document
cp tests/fixtures/contracts/v0.1/recipe.clip.json \
  fuzz/corpus/recipe_document/
cargo +nightly fuzz run recipe_document
```

Pull requests receive a five-minute ClusterFuzzLite code-change run. A
15-minute batch run executes weekly on `main`. Both use the checked-in example
recipes as initial corpus material and publish machine-readable findings to
GitHub code scanning.
Each code-changing `main` update also saves a comparison build so later pull
requests can distinguish newly introduced crashes. The accumulated corpus is
pruned after every weekly batch.

When a crash is found, retain the minimized input privately until it is known
not to expose sensitive media paths or metadata. Add a deterministic regression
test before fixing the underlying parser or validation defect. Report
security-sensitive crashes through the private process in
[SECURITY.md](SECURITY.md).
