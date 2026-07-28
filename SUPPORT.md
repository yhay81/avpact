# Support

## Where to ask

- Use [GitHub Discussions](https://github.com/yhay81/avpact/discussions) for
  installation help, workflow questions, and design exploration.
- Use a structured [GitHub issue](https://github.com/yhay81/avpact/issues/new/choose)
  for reproducible bugs or scoped feature requests.
- Follow [SECURITY.md](SECURITY.md) for vulnerabilities. Do not disclose a
  security-sensitive recipe, path, log, or media file in a public issue.

AVPact is maintained by volunteers. There is no guaranteed response time, but
reports that include a minimal synthetic reproduction and complete version
information are the easiest to investigate.

## Supported environment

The latest tagged pre-1.0 release is supported on:

- Linux x86-64;
- macOS x86-64 and Apple silicon;
- Windows x86-64;
- Rust 1.85 or newer when building from source;
- FFmpeg and FFprobe builds that expose the capabilities required by the
  selected operation.

Run these commands when preparing a report:

```bash
avpact --version
avpact capabilities --format json
ffmpeg -version
ffprobe -version
```

Redact user names, absolute paths, metadata, and unrelated codecs or filters
before posting capability output.

## Scope

The project cannot provide support for arbitrary FFmpeg arguments, network
inputs, DRM circumvention, damaged private media that cannot be shared as a
synthetic reproduction, or backend vulnerabilities that require an FFmpeg
update. See [docs/OPERATIONS.md](docs/OPERATIONS.md) for supported operations
and explicit limitations.
