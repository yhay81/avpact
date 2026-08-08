# Security policy

## Supported versions

AVPact is pre-1.0. Security fixes are applied to the latest tagged release.
Older pre-1.0 releases are not supported after a newer release is available.

| Version | Supported |
| --- | --- |
| Latest tagged release | Yes |
| Older pre-1.0 releases | No |
| Unreleased development builds | Best effort |

## Reporting a vulnerability

Use
[GitHub private vulnerability reporting](https://github.com/yhay81/avpact/security/advisories/new).
Please do not open a public issue for command execution, path handling,
temporary-file, overwrite, cancellation, resource-limit, receipt-integrity, or
media-parser vulnerabilities.

Include the AVPact version, operating system, FFmpeg/FFprobe version, recipe or
plan with sensitive paths redacted, observed behavior, and a minimal
reproduction when possible.

You should receive an acknowledgement within 7 days. The maintainer will
validate the report, agree on disclosure timing, prepare a fix and regression
test, and publish a GitHub Security Advisory when appropriate. Response targets
are goals rather than a service-level agreement for this volunteer project.

Do not attach private media unless the maintainer explicitly requests a secure
transfer. Synthetic reproductions are strongly preferred.

## Security boundaries

- AVPact invokes FFmpeg and FFprobe with argument arrays, never shell command
  strings.
- A plan is executable authority, not a cryptographic signature from a trusted
  author. Apply validates that its argv can be reconstructed from the typed
  plan before execution.
- A v0.2 receipt identifier is an unkeyed SHA-256 digest over all recorded
  evidence. Readers detect content mutation and receipt-store key substitution,
  but the digest does not authenticate an author. Legacy v0.1 receipts bind
  fewer fields and remain readable only for compatibility; see
  [receipt integrity](docs/RECEIPT_INTEGRITY.md).
- Inputs and outputs are local files. Network protocols and DRM circumvention
  are out of scope.
- Codec and filter vulnerabilities in the selected FFmpeg build remain part of
  the trusted computing base.
- Release archives are produced only by the tagged GitHub Actions workflow.
  Consumers should verify checksums and provenance as described in
  [RELEASING.md](RELEASING.md).

## Dependency and disclosure policy

Dependabot monitors Rust and GitHub Actions dependencies. CI audits `Cargo.lock`
against RustSec advisories. A dependency advisory is evaluated for reachability
and impact; an available compatible security update is preferred over an
indefinite advisory exception.

Pull requests are checked with GitHub Dependency Review and fail when they
introduce a dependency with a known moderate-or-higher-severity vulnerability.
A weekly OpenSSF Scorecard analysis publishes authenticated results and uploads
SARIF findings to GitHub code scanning. CodeQL default setup analyzes Rust and
workflow sources with extended security queries. ClusterFuzzLite runs the
production recipe parser and semantic validator on every code-changing pull
request and in a longer weekly AddressSanitizer batch; see
[FUZZING.md](FUZZING.md).

Public disclosure normally follows a fixed release. If a report affects
FFmpeg, GitHub Actions, or another upstream project, AVPact will coordinate with
that project before disclosure when practical.
