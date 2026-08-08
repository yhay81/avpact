# AVPact contract compatibility corpus

This corpus freezes versioned documents emitted or accepted by AVPact. Current
and future readers must continue to accept every golden document, preserve its
serialized representation, and reject every mutation declared by the
corresponding manifest.

- `v0.1/` preserves the original recipe, plan, and legacy receipt contracts.
- `v0.2/` pins the content-addressed receipt contract and its complete-evidence
  integrity mutations.

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
fail-closed mutations. Raw-text mutations preserve otherwise-valid identities
while exercising ambiguous JSON such as duplicate keys. `.gitattributes`
forces LF bytes for this directory so the byte-level digests are identical on
Windows, macOS, and Linux.
