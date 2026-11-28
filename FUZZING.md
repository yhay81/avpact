# Fuzzing AVPact

Maintainers can fuzz recipe and receipt inputs locally with AddressSanitizer.
The `recipe_document` target exercises the same bounded JSON
parser and semantic validator used before AVPact inspects media or invokes a
backend. The `receipt_document` target exercises the bounded reader, recursive
duplicate-key rejection, typed schema, content-derived identifier, and semantic
receipt checks.

Install a current nightly toolchain and the pinned local runner, then run:

```bash
cargo install cargo-fuzz --version 0.13.2 --locked
mkdir -p fuzz/corpus/recipe_document
cp tests/fixtures/contracts/v0.1/recipe.clip.json \
  fuzz/corpus/recipe_document/
cargo +nightly fuzz run recipe_document -- -max_total_time=300

mkdir -p fuzz/corpus/receipt_document
cp tests/fixtures/contracts/v0.2/receipt.clip.json \
  fuzz/corpus/receipt_document/
cargo +nightly fuzz run receipt_document -- -max_total_time=300
```

These runs are optional. Each command spends up to five minutes fuzzing after
the build. Adjust the duration for a longer session. The checked-in contract
fixtures also run in ordinary pull-request CI.

When a crash is found, retain the minimized input privately until it is known
not to expose sensitive media paths or metadata. Add a deterministic regression
test before fixing the underlying parser or validation defect. Report
security-sensitive crashes through the private process in
[SECURITY.md](SECURITY.md).
