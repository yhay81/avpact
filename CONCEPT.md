# AVPact concept

## One-line thesis

AVPact turns typed media intentions into inspectable execution plans,
verified outputs, and reproducible transformation receipts.

## Problem

FFmpeg is powerful but exposes a large, order-sensitive command language.
Agents can generate plausible commands that:

- select the wrong stream or time base;
- silently change aspect ratio, frame rate, color, or audio layout;
- overwrite inputs or partial outputs;
- use host-specific defaults;
- produce a file that exists but violates the requested constraints;
- flood context with progress and diagnostic output.

Human-oriented wrappers simplify a few tasks but rarely expose a complete plan
and post-render verification contract.

## Target users and jobs

- Agents preparing video, audio, subtitles, and thumbnails.
- Developers building media features without embedding FFmpeg expertise.
- Content teams automating repeatable transformations.
- Test and release systems validating generated media.

The primary job is: **declare a media outcome and constraints, inspect the exact
plan, execute it safely, and verify the resulting artifact.**

## Product principles

1. Typed intent is the primary interface; raw filter graphs are an escape hatch.
2. Planning never modifies media.
3. Inputs and outputs are identified by digest and probed properties.
4. Stream selection and defaults are explicit.
5. Output validation is mandatory for successful completion.
6. Progress is bounded and structured.
7. The exact backend command remains inspectable.

## Proposed command contract

```text
avpact schema --brief --format json
avpact inspect input.mp4 --format json
avpact plan recipe.json --out plan.json --format json
avpact apply plan.json --progress ndjson --format json
avpact verify output.mp4 --against plan.json --format json
avpact receipt show <receipt-id> --format json
```

## Recipe model

A recipe contains typed operations such as:

- clip by time range;
- concatenate compatible sources;
- transcode audio or video;
- resize, fit, crop, pad, and rotate;
- extract or replace audio;
- loudness normalization;
- generate thumbnails or contact sheets;
- attach or burn subtitles;
- map, remove, or preserve selected streams.

It also declares output constraints: container, codec family, dimensions, aspect
ratio, duration tolerance, frame rate policy, audio channels, loudness target,
subtitle policy, metadata allowlist, size budget, and overwrite behavior.

## Plan model

The immutable plan records:

- input paths, digests, sizes, and probed streams;
- normalized recipe and constraint digests;
- chosen streams and selection reasons;
- exact codec, filter, mapping, and container decisions;
- exact FFmpeg argument vector;
- backend and codec versions;
- temporary files and atomic-output strategy;
- estimated duration, disk use, and resource class;
- expected output properties and verification checks;
- warnings, lossy decisions, and unsupported requests.

The plan can be reviewed without executing FFmpeg.

## Apply and verification

Apply writes to a temporary destination and publishes atomically only after:

- the backend exits successfully;
- the output can be parsed;
- required streams exist;
- dimensions, duration, codecs, layout, and other constraints pass;
- the output is distinct from protected inputs;
- the final digest is recorded.

The receipt contains the plan digest, backend arguments and version, timings,
bounded warnings, output digest and properties, and every verification result.
Current v0.2 receipts derive their full SHA-256 identifier from all of that
evidence and the publication result. This detects mutation when the expected
identifier is trusted; it is not an author signature.

## Initial scope

Version 0.1 will support:

- FFmpeg and FFprobe through argument arrays;
- local files;
- clip, concatenate, transcode, resize/crop/pad, audio extraction and
  normalization, thumbnails, and subtitle burn-in;
- JSON recipes, plans, receipts, and verification reports;
- structured progress and cancellation;
- atomic local output where the filesystem permits it.

## Non-goals

- A nonlinear timeline editor.
- Generative image, video, or audio models.
- Media hosting or distributed rendering.
- Arbitrary FFmpeg syntax as the normal agent contract.
- DRM removal or circumvention.
- Claiming perceptual quality without a declared measurement.

## Differentiation and defensibility

AVPact does not hide FFmpeg; it makes its decisions explicit and verifiable.
The typed recipe schema, deterministic planner, compatibility knowledge, and
fixture corpus can become a durable layer used by both agents and applications.

## Success measures

- Recipe success rate across a diverse media fixture corpus.
- Output-constraint violation and silent-default rates.
- Plan reproducibility across supported environments.
- Median tokens and retries per media task.
- Verification coverage for each operation.
- Adoption as a backend by agent and automation frameworks.

## Key risks and open questions

- Codec availability and behavior differ by FFmpeg build and platform.
- Deterministic byte output is often unrealistic across codec versions.
- Media edge cases grow quickly with container and stream combinations.
- Temporary disk requirements can exceed estimates.
- Raw-filter escape hatches can undermine the typed safety model.

AVPact should promise reproducible intent and verified properties, not
universal byte-for-byte output identity.
