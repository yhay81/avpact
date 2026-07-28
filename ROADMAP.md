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

Done when an end-to-end clip either publishes a verified output and receipt or
leaves the requested destination untouched.

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

## Current environment note

The initial development environment has FFmpeg and FFprobe 8.1.2. Unit tests
remain backend-independent, while the generated-media integration test runs
when both executables are present and skips cleanly when they are not.
