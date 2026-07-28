# AVPact fixture corpus

The media corpus is generated at test time from FFmpeg's deterministic `lavfi`
sources. No third-party audiovisual work is checked into this repository.

| Fixture ID | Shape | Purpose |
| --- | --- | --- |
| `av_mpeg4_aac_mp4` | 160×90, 10 fps, mono AAC, 500 ms | Common audio/video inspection and every transform |
| `video_only_mpeg4_mp4` | 128×72, 12 fps, 300 ms | Optional-audio policy |
| `audio_only_pcm_wav` | 48 kHz mono PCM, 300 ms | Audio-only inspection |
| `single_mjpeg_jpeg` | 96×54 | Still-image inspection |
| `subtitle_srt` | One UTF-8 cue | Subtitle discovery and burn-in |
| `mismatched_video` | 120×120 | Concatenation compatibility rejection |

Every generated signal is a synthetic test pattern, tone, or text string
created by this project. The corpus metadata in `METADATA.json` is licensed
under CC0-1.0; repository source code remains MIT licensed.

Tests skip backend-dependent cases only when FFmpeg/FFprobe or a declared
optional capability is absent. CI records the capability matrix on Linux,
macOS, and Windows, and Linux requires subtitle-filter support.

Versioned recipe, plan, receipt, and fail-closed mutation fixtures live in
[`contracts/`](contracts/README.md). Unlike the generated media corpus, these
documents are immutable compatibility evidence and remain checked in when a
new contract version is introduced.
