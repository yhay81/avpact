# Security policy

## Supported versions

AVPact is pre-1.0. Security fixes are applied to the latest tagged release.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting feature for this repository.
Please do not open a public issue for command execution, path handling,
temporary-file, overwrite, or media-parser vulnerabilities.

Include the AVPact version, operating system, FFmpeg/FFprobe version, recipe or
plan with sensitive paths redacted, observed behavior, and a minimal
reproduction when possible.

## Security boundaries

- AVPact invokes FFmpeg and FFprobe with argument arrays, never shell command
  strings.
- A plan is executable authority, not a cryptographic signature from a trusted
  author. Apply validates that its argv can be reconstructed from the typed
  plan before execution.
- Inputs and outputs are local files. Network protocols and DRM circumvention
  are out of scope.
- Codec and filter vulnerabilities in the selected FFmpeg build remain part of
  the trusted computing base.
