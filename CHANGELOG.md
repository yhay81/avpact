# Changelog

All notable changes to AVPact are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use
semantic versioning.

## [Unreleased]

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
