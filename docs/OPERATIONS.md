# AVPact 0.1 operations

All recipe paths are resolved relative to the recipe file. Planning probes and
hashes every input but does not create or modify media. Apply revalidates those
identities before starting FFmpeg.

Common constraints are optional and have bounded defaults:

```json
{
  "constraints": {
    "overwrite": "deny",
    "duration_tolerance_ms": 100,
    "max_output_bytes": 10737418240,
    "max_temporary_bytes": 12884901888,
    "max_runtime_ms": 14400000
  }
}
```

Overwrite is deliberately limited to `deny` in 0.1. All transforms drop
unselected streams and input metadata, and record those decisions as plan
warnings. The `web` target uses H.264/AAC, preserves a known input frame rate,
normalizes video to square pixels, and verifies the exact output stream counts.
Still-image operations use lossy JPEG. Every operation records its lossy
encoding decision; metadata preservation has an empty allowlist in 0.1.

## Clip

```json
{
  "type": "clip",
  "input": "../input.mp4",
  "output": "../clip.mp4",
  "start_ms": 10000,
  "end_ms": 30000
}
```

Requires a known input duration and a video stream. It selects the default (or
first) video and optional audio stream, then uses an exact-transcode policy to
H.264/AAC MP4.

## Transcode

```json
{
  "type": "transcode",
  "input": "../input.mov",
  "output": "../web.mp4"
}
```

Produces H.264/AAC MP4 while retaining the selected input dimensions and known
duration. Additional streams are dropped explicitly.

## Resize, fit, crop, pad, and rotate

```json
{
  "type": "resize",
  "input": "../input.mp4",
  "output": "../resized.mp4",
  "width": 1280,
  "height": 720,
  "mode": "pad",
  "rotation": "none"
}
```

`mode` is `stretch`, `fit`, `crop`, or `pad`. `rotation` is `none`,
`clockwise90`, `clockwise180`, or `counter_clockwise90`. Requested dimensions
must be even values from 2 through 16384. Fit preserves aspect ratio within the
box; crop fills then center-crops; pad fits then centers on a black canvas.

## Extract audio

```json
{
  "type": "extract_audio",
  "input": "../input.mp4",
  "output": "../audio.m4a"
}
```

Requires an audio stream and produces 192 kbit/s AAC in M4A with no video.

## Normalize audio

```json
{
  "type": "normalize_audio",
  "input": "../input.mp4",
  "output": "../normalized.m4a",
  "target_lufs": -14,
  "tolerance_lu_x100": 100
}
```

The integrated target is an integer from -70 through -5 LUFS. Apply uses the
FFmpeg `loudnorm` filter, then performs a separate measurement pass. The
receipt contains the measured integrated loudness and pass/fail result.

## Concatenate

```json
{
  "type": "concatenate",
  "inputs": ["../part-a.mp4", "../part-b.mp4"],
  "output": "../joined.mp4"
}
```

Accepts 2 through 64 video inputs. Dimensions and known frame rates must match.
Either every input has audio or none does. Timestamps, sample rate, layout,
pixel aspect, and pixel format are normalized inside an explicit concat filter
graph.

## Thumbnail

```json
{
  "type": "thumbnail",
  "input": "../input.mp4",
  "output": "../thumbnail.jpg",
  "at_ms": 10000,
  "width": 640
}
```

Produces one aspect-preserving JPEG. Width must be even and from 2 through
8192; the timestamp must be within a known input duration when one is present.

## Contact sheet

```json
{
  "type": "contact_sheet",
  "input": "../input.mp4",
  "output": "../sheet.jpg",
  "interval_ms": 5000,
  "columns": 4,
  "rows": 3,
  "width": 1280
}
```

Produces one JPEG with at most 100 cells. Interval is 100 through 3,600,000 ms,
and the requested width must produce an even cell width.

## Burn subtitles

```json
{
  "type": "burn_subtitles",
  "input": "../input.mp4",
  "subtitles": "../captions.srt",
  "output": "../subtitled.mp4"
}
```

The subtitle file is separately hashed and probed, then rendered into H.264
video pixels. The output contains no subtitle stream. This operation requires
an FFmpeg build with the `subtitles` filter (normally libass); check
`avpact capabilities` before planning.

## Capability-dependent behavior

Planning rejects a request when a required encoder or filter is absent. AVPact
0.1 does not silently choose a different codec, alter overwrite behavior,
accept arbitrary FFmpeg syntax, preserve every metadata field, or promise
byte-identical results across backend versions.
