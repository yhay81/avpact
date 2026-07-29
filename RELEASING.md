# Releasing AVPact

Only a release manager identified in [GOVERNANCE.md](GOVERNANCE.md) may create
a release tag.

1. Confirm the version is not already published and that `CHANGELOG.md`,
   `Cargo.toml`, and `Cargo.lock` use the same version.
2. Confirm the release commit is on `main`, the worktree is clean, and all
   required CI checks pass without exceptions.
3. Run the local release gate:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets --locked -- -D warnings
   cargo test --all-targets --locked
   cargo package --locked --allow-dirty
   cargo build --release --locked
   ```

4. Confirm the Linux, macOS, and Windows CI matrix, declared MSRV, dependency
   audit, generated schemas, and documented examples are green.
5. Create and push a signed annotated tag:

   ```bash
   git tag -s v0.3.0 -m "AVPact 0.3.0"
   git push origin v0.3.0
   ```

6. The release workflow builds native archives, includes shell completions and
   documentation, generates a CycloneDX SBOM and `SHA256SUMS`, creates the
   GitHub release, and publishes GitHub/Sigstore build provenance and SBOM
   attestations. Each archive includes a downloadable `.intoto.jsonl`
   provenance bundle for local verification.
7. From a clean temporary directory, verify a downloaded archive:

   ```bash
   sha256sum --check SHA256SUMS
   gh attestation verify avpact-v0.3.0-linux-x86_64.tar.gz \
     --repo yhay81/avpact
   gh attestation verify avpact-v0.3.0-linux-x86_64.tar.gz \
     --repo yhay81/avpact \
     --bundle avpact-v0.3.0-linux-x86_64.tar.gz.intoto.jsonl \
     --signer-workflow yhay81/avpact/.github/workflows/release.yml
   gh attestation verify avpact-v0.3.0-linux-x86_64.tar.gz \
     --repo yhay81/avpact \
     --predicate-type https://cyclonedx.org/bom
   ```

8. Extract every archive, run `avpact --version`, generate a completion script,
   and execute `avpact schema --brief --format json`. On at least one supported
   platform with FFmpeg, also run a generated-media inspect/plan/apply/verify
   smoke test.
9. Confirm that the release notes link to the changelog, installation
   instructions, checksums, SBOM, and security reporting policy.

## crates.io

The first crates.io release must be published manually because Trusted
Publishing can only be configured after the crate exists. From the exact signed
release commit, repeat `cargo publish --dry-run --locked`, review
`cargo package --list --locked`, then publish:

```bash
cargo publish --locked
```

Use a Cargo credential provider backed by the operating-system credential
store. Never put a crates.io token in Git, workflow YAML, logs, or a
repository-level Actions secret. If Cargo times out after upload, check the
crates.io page and index before retrying; an accepted version is immutable.

After the first manual release:

1. Add the crate's Trusted Publisher in crates.io, restricted to
   `yhay81/avpact`, the dedicated publish workflow filename, and the protected
   `crates-io` GitHub environment.
2. Add that workflow only after the mapping exists. Grant only
   `contents: read` and `id-token: write`, pin every action to an immutable
   commit, exchange OIDC with `rust-lang/crates-io-auth-action`, and run
   `cargo publish --locked`.
3. Remove any temporary API token, verify registry ownership and account
   recovery without recording secrets, and require environment approval for
   every publish.
4. Install the exact version from crates.io in a clean environment and repeat
   the CLI smoke checks.

If any release verification fails, do not reuse the version or move the tag;
document the failure and publish a new patch release.
