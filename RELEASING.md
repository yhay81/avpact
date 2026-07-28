# Releasing AVPact

1. Confirm `CHANGELOG.md` and `Cargo.toml` use the same version.
2. Run the local release gate:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets --locked -- -D warnings
   cargo test --all-targets --locked
   cargo package --locked --allow-dirty
   cargo build --release --locked
   ```

3. Confirm the Linux, macOS, and Windows CI matrix is green.
4. Create and push an annotated tag:

   ```bash
   git tag -s v0.1.0 -m "AVPact 0.1.0"
   git push origin v0.1.0
   ```

5. The release workflow builds native archives, includes shell completions and
   documentation, generates `SHA256SUMS`, creates the GitHub release, and
   publishes GitHub/Sigstore build provenance attestations.
6. Verify a downloaded archive:

   ```bash
   sha256sum --check SHA256SUMS
   gh attestation verify avpact-v0.1.0-linux-x86_64.tar.gz \
     --repo yhay81/avpact
   ```

Publishing the crate to crates.io is intentionally manual until package
ownership and registry credentials are configured:

```bash
cargo publish --locked
```
