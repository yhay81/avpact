#!/bin/bash -eu

cd "$SRC/avpact"
cargo fuzz build -O --debug-assertions

fuzz_output="fuzz/target/x86_64-unknown-linux-gnu/release"
for source in fuzz/fuzz_targets/*.rs; do
    target="$(basename "${source%.*}")"
    cp "$fuzz_output/$target" "$OUT/$target"
done

zip -q -j "$OUT/recipe_document_seed_corpus.zip" \
    examples/*.recipe.json \
    tests/fixtures/contracts/v0.1/recipe.clip.json

zip -q -j "$OUT/receipt_document_seed_corpus.zip" \
    tests/fixtures/contracts/v0.2/receipt.clip.json
