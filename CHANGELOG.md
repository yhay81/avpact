# Changelog

All notable changes to AVPact are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use
semantic versioning.

## [Unreleased]

### Changed

- Upgraded `sha2` to 0.11 and centralized lowercase hexadecimal encoding while
  preserving all existing digest and receipt identifier contracts.
- Defined measurable v1.0 compatibility, correctness, security, performance,
  delivery, maintenance, contribution, and repeat-adoption gates.

### Added

- Added a digest-pinned v0.1 recipe, plan, and receipt compatibility corpus
  with exact round-trip checks, full semantic validation, and nine declared
  fail-closed mutation cases.
- Added a reproducible synthetic-media performance harness covering contracts,
  inspect, plan, apply, progress, receipt, verification, and cleanup with raw
  runner and FFmpeg identity retained for 90 days.

## [0.1.0] - 2026-07-28

### Added

- Versioned JSON contracts and JSON Schema for recipes, inspections, plans,
  progress, verification reports, receipts, errors, and capabilities.
- Typed operations for clip, transcode, resize/fit/crop/pad/rotation, audio
  extraction, loudness normalization, concatenation, thumbnails, contact
  sheets, and subtitle burn-in.
- Deterministic FFmpeg argument planning with explicit stream selection,
  normalized recipe/constraint digests, and pinned FFmpeg build/library plus
  FFprobe versions.
- Input digest revalidation, bounded progress, cooperative cancellation,
  process-tree cleanup, resource budgets, exact stream/aspect/frame-rate
  verification, no-clobber atomic publication, and receipt-failure rollback.
- Durable local receipt store and `receipt show`.
- Linux, macOS, and Windows CI plus release archives, checksums, completions,
  CycloneDX SBOMs, and GitHub build provenance and SBOM attestations.
- Contributor, governance, support, conduct, security-response, and adoption
  policies with structured issue and pull-request templates.
- Automated RustSec audits and grouped Dependabot updates for Cargo and GitHub
  Actions dependencies.

[Unreleased]: https://github.com/yhay81/avpact/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yhay81/avpact/releases/tag/v0.1.0
