# Governance

AVPact is maintained in public. This document describes how decisions are made
while the project has a small maintainer group.

## Roles

- **Contributors** propose issues, documentation, tests, code, and reviews.
- **Maintainers** triage reports, review changes, define releases, manage
  security responses, and protect the compatibility and safety contracts.
- **Release managers** are maintainers authorized to create signed tags and
  trigger the release workflow.

The repository owner is the current maintainer and release manager. New
maintainers may be added after sustained, constructive contributions and a
demonstrated understanding of AVPact's safety boundaries.

## Decision process

Small, reversible changes are decided through pull-request review. Public
contract changes, new operations, dependency policy changes, and changes to the
security boundary start with an issue and remain open for feedback before
implementation.

Decisions favor:

1. safety and verifiability;
2. compatibility and predictable automation;
3. a small, typed surface over arbitrary backend passthrough;
4. evidence from tests, fixtures, and real user workflows;
5. reversible implementation choices.

If consensus is not reached, a maintainer records the decision and rationale in
the issue or pull request. Security-sensitive details may be handled privately
until coordinated disclosure is safe.

## Changes and releases

Pull requests from contributors need at least one maintainer approval.
Maintainer-authored pull requests need a recorded self-review and all required
checks. Maintainers do not bypass failing checks for a release. Release
requirements are defined in [RELEASING.md](RELEASING.md), and supported versions
are defined in [SECURITY.md](SECURITY.md).

## Project health

Maintainers periodically review dependency freshness, unanswered issues,
unsupported platforms, release reproducibility, security reports, and the
adoption evidence in [ADOPTERS.md](ADOPTERS.md). Governance will be revised as
the contributor base grows.
