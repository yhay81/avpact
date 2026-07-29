# AVPact roadmap

AVPact 0.1 is built as a sequence of vertical, verifiable slices. Each slice
must preserve the product contract: planning is read-only, backend commands are
argument arrays, publication is atomic where supported, and a successful apply
includes output verification.

## Implementation decisions

- **Language:** Rust, producing one cross-platform CLI binary.
- **Serialization:** versioned JSON documents with stable field names.
- **Backend boundary:** FFmpeg and FFprobe are child processes invoked only with
  argument arrays. Shell command strings are never constructed.
- **Identity:** local inputs and durable documents use SHA-256 digests.
- **Determinism:** plans contain no creation timestamp or random identifier.
  Their IDs are derived from canonical plan content.
- **Testing:** parsers and planners use checked-in fixtures; backend execution
  tests use generated media only when FFmpeg is available.
- **Initial operation:** an exact-duration clip is the first end-to-end slice.
  More operations are added only after apply, verification, and receipts work.

These decisions are intentionally narrow. Codec policy, progress event cadence,
and cross-filesystem publication remain explicit design tasks rather than
implicit backend defaults.

## Ordered work

### 1. Contract and project foundation

- [x] Record implementation decisions and ordered milestones.
- [x] Create the Rust package and CLI entry point.
- [x] Define bounded, structured errors and versioned report types.
- [x] Publish JSON Schema for every accepted or emitted document.

Done when the package builds, tests pass, and every implemented command emits a
versioned JSON document on success.

### 2. Read-only inspection

- [x] Hash and identify a local input without modifying it.
- [x] run FFprobe through an argument array;
- [x] normalize common container, stream, duration, video, and audio fields;
- [x] retain bounded backend diagnostics on failure;
- [x] add an optional generated-media integration test when FFmpeg is present.

Done when the same fixture produces the same normalized report, aside from its
canonical source path, and missing or malformed inputs fail structurally.

### 3. Clip planning

- [x] Define and validate the versioned recipe document.
- [x] Reject input/output aliasing and unsafe overwrite behavior.
- [x] Make stream selection and clip accuracy policy explicit.
- [x] Compile a deterministic FFmpeg argument vector.
- [x] Record expected properties and verification checks.

Done when planning performs no media mutation and snapshot tests cover every
argument and warning in the resulting plan.

### 4. Apply, verify, and receipt

- [x] Revalidate input identity immediately before execution.
- [x] Encode to a destination-adjacent temporary file.
- [x] Emit bounded NDJSON progress.
- [x] Support cooperative cancellation and process-tree cleanup.
- [x] Probe and verify the temporary output against the plan.
- [x] Publish atomically only after successful verification.
- [x] Persist a receipt containing plan, backend, timing, output, and checks.
- [x] Store receipts by durable ID and support `receipt show`.

Done when an end-to-end clip either publishes a verified output and receipt,
leaves the requested destination untouched before publication, or retains the
verified output plus explicit recovery evidence when final receipt persistence
fails.

### 5. Operation expansion

Add one operation at a time in this order:

1. [x] transcode;
2. [x] resize, fit, crop, pad, and rotation;
3. [x] audio extraction;
4. [x] measured loudness normalization;
5. [x] concatenate compatible inputs;
6. [x] thumbnails and contact sheets;
7. [x] subtitle burn-in with explicit capability detection.

Each operation requires recipe validation, deterministic planning, verification
coverage, fixture coverage, and documented unsupported cases.

### 6. Release readiness

- [x] Linux, macOS, and Windows CI definitions.
- [x] Capability detection across representative FFmpeg builds.
- [x] Diverse synthetic media fixture corpus with licensing metadata.
- [x] Resource, temporary-disk, runtime, and output-size budgets.
- [x] Native release archives, shell completions, checksums, and provenance.
- [x] Reproducible end-to-end media workflow and contract performance baseline
  with raw hosted-runner measurements.

## v1.0 quality criteria

AVPact reaches v1.0 only when every gate below is supported by published,
reproducible evidence. A larger operation list, download count, or star count
does not substitute for these gates.

### Product and compatibility

- The CLI, recipe, plan, receipt, inspection, capabilities, and error contracts
  remain compatible across at least two released pre-1.0 minor versions.
- Golden documents from every supported contract version are accepted by the
  current reader or have a tested migration command and migration guide.
- Every supported operation has a documented codec, stream-selection,
  overwrite, temporary-file, and verification policy.
- Backend capability differences never cause a silent operation, codec, or
  accuracy downgrade; the plan records every authorized fallback.

Current evidence: v0.2 and v0.3 provide two released compatibility cycles. The
current reader accepts the digest-pinned v0.1 recipe, plan, and receipt corpus
byte-for-byte and emits content-addressed v0.2 receipts. Both receipt versions
have golden documents and fail-closed mutation cases. The v0.1 format remains
readable as legacy evidence; it cannot retroactively gain v0.2's complete-field
binding, so no automatic migration is claimed. Future contract versions must
add their own golden documents and an explicit migration or no-migration
decision.

### Correctness and security

- The published adversarial corpus has 100% detection of input identity
  changes, current-format receipt mutations relative to a retained expected
  identifier, output-verification failures, unsafe input/output aliasing, and
  attempts to overwrite an existing destination.
- Every forced failure before publication leaves the requested destination
  byte-identical or absent and leaves no unbounded temporary artifact. A forced
  receipt failure after publication retains the verified output, never deletes
  a path after a separate identity check, and emits bounded, machine-actionable
  reconciliation evidence.
- Each operation meets its published duration, geometry, stream, loudness, and
  content-verification tolerances across the supported FFmpeg matrix.
- An independent security review covers path and symlink handling, temporary
  publication, cancellation races, process-tree cleanup, backend argument
  construction, receipt integrity, and diagnostic redaction; all critical and
  high findings are resolved.
- No known critical or high-severity vulnerability is open at release time.

### Performance and bounds

- Planning and contract generation remain below 250 ms p95 on the published
  fixture corpus, excluding input hashing and FFprobe/FFmpeg time.
- AVPact parent-process peak memory remains below 256 MiB for every published
  bounded fixture; backend process memory is measured and reported separately.
- Backend diagnostics, progress events, receipt size, runtime, output size, and
  temporary-disk use never exceed their configured bounds without an explicit
  structured failure.
- Benchmark methodology, runner image, FFmpeg build, raw measurements, and
  regression thresholds are versioned with the repository.

Current evidence enforces a stronger 250 ms p95 end-to-end planning bound that
includes the excluded hashing, FFprobe, and FFmpeg capability work. The CLI
process-tree RSS bound includes child processes and therefore upper-bounds the
AVPact parent; the exact planned FFmpeg invocation is also measured and
reported independently. One warm-up and 20 raw samples on Ubuntu 24.04 feed
nearest-rank p95 and versioned noise-aware regression limits.

### Delivery and maintenance

- Required CI stays green on Linux, macOS, and Windows for 30 consecutive days
  before the v1.0 tag.
- Releases originate only from protected `main` and signed annotated tags; all
  native archives have verified checksums, GitHub-hosted provenance, and a
  CycloneDX SBOM attestation.
- The release runbook is successfully exercised by two maintainers, or
  governance records the single-maintainer continuity risk and a tested
  recovery procedure.
- Security reports are acknowledged within 3 business days and receive an
  initial assessment within 7.

### Adoption evidence

- At least three independent users or teams are recorded in
  [ADOPTERS.md](ADOPTERS.md) with a real media workflow and an outcome that
  verification or a safe refusal improved.
- At least two adopters report repeat use separated by 30 days.
- At least one public integration demonstrates plan review plus verified apply,
  rather than installation or inspection alone.
- At least one non-maintainer issue, discussion, documentation change, fixture,
  test, or code contribution is resolved and credited.

Maintainer-authored fixtures, automated downloads, stars, and synthetic
accounts cannot satisfy adoption gates.

## Current environment note

The initial development environment has FFmpeg and FFprobe 8.1.2. Unit tests
remain backend-independent, while the generated-media integration test runs
when both executables are present and skips cleanly when they are not.
