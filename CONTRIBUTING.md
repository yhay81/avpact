# Contributing to AVPact

Thank you for helping make media automation safer and easier to inspect.
Contributions of code, tests, documentation, fixture improvements, and
reproducible bug reports are welcome.

## Before opening an issue

- Use GitHub Discussions for usage questions and design exploration.
- Search existing issues before opening a new one.
- Report security-sensitive behavior privately as described in
  [SECURITY.md](SECURITY.md).
- Do not attach private media. Prefer a minimal synthetic input and redact local
  paths, metadata, and backend output before posting.

## Development setup

AVPact requires Rust 1.85 or newer. End-to-end tests also require FFmpeg and
FFprobe on `PATH`.

```bash
git clone https://github.com/yhay81/avpact.git
cd avpact
cargo test --all-targets --locked
```

Unit and fixture tests run without FFmpeg. Generated-media integration tests run
when FFmpeg and FFprobe are available and skip otherwise.

## Making a change

1. Open an issue first for a new operation, public contract change, or
   architecture change. Small fixes do not need a separate issue.
2. Keep the change focused. Avoid unrelated formatting or dependency updates.
3. Add tests at the lowest useful layer:
   - unit tests for validation, canonicalization, and plan construction;
   - checked-in fixtures for parser and schema behavior;
   - generated-media integration tests for apply, verification, and cleanup.
4. Update schemas, examples, operational documentation, and the changelog when
   behavior visible to users changes.
5. Run the local quality gate:

   ```bash
   cargo fmt --check
   cargo clippy --all-targets --locked -- -D warnings
   cargo test --all-targets --locked
   cargo package --locked --allow-dirty
   ```

For execution changes, also test the failure path. A failed or cancelled apply
must leave the requested destination untouched and clean up its child process
tree and temporary output.

## Public contract changes

Recipes, plans, receipts, verification reports, progress events, capability
reports, errors, exit codes, and generated JSON Schema are public interfaces.
Changes to them must:

- preserve compatibility within a stable major version, or be explicitly
  documented as a breaking pre-1.0 change;
- remain bounded and machine-readable;
- include migration notes and before/after examples;
- keep planning read-only and backend commands represented as argument arrays.

## Pull requests

Pull requests should explain the user problem, the chosen scope, and how the
change was verified. The PR template contains the release-safety checklist.
Draft PRs are welcome for early feedback.

By contributing, you agree that your contribution is licensed under the MIT
license in this repository and that you will follow the
[Code of Conduct](CODE_OF_CONDUCT.md).
