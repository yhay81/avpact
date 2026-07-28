# AVPact contract compatibility corpus

This corpus freezes documents emitted or accepted by AVPact 0.1. Current and
future readers must continue to accept every document in `v0.1/`, preserve its
serialized representation, and reject every mutation declared in
`v0.1/manifest.json`.

The fixtures use synthetic identities and platform-neutral relative paths.
They do not refer to third-party media and are covered by the repository's MIT
license.

When a contract intentionally changes:

1. keep the existing version directory unchanged;
2. add a new version directory and manifest;
3. teach the current reader to accept the old documents or provide a tested
   migration command;
4. document the compatibility decision and migration in the changelog.

The integration test verifies fixture SHA-256 digests, exact pretty-JSON
round-trips, semantic plan and receipt validation, and the declared
fail-closed mutations.
