# RenderPact

Declarative, inspectable, and verifiable media transformations.

> Status: concept stage. The proposed engine uses FFmpeg but does not expose its full option surface as the primary contract.

RenderPact compiles a small set of media intents into a deterministic execution plan. The plan records streams, codecs, filters, expected outputs, resource limits, and verification checks before any transformation starts.

```bash
renderpact inspect input.mp4
renderpact plan clip input.mp4 --from 00:10 --to 00:30
renderpact plan transcode input.mov --target web
renderpact apply plan_01J...
renderpact verify rcpt_01J...
```

## Why

FFmpeg is powerful but its option space is difficult to predict safely. Agents should choose a typed media intent, not synthesize a fragile shell command from memory.

## Product principles

- Declarative recipes are the source of truth.
- Inspect and plan before compute-heavy mutation.
- No shell interpolation.
- Deterministic encoder and filter selection is visible in the plan.
- Progress is emitted as bounded NDJSON events.
- Output properties are verified after encoding.
- The compiled FFmpeg argv remains inspectable.

## Initial scope

Inspect, clip, concatenate, transcode, resize/crop, extract audio, normalize audio, create thumbnails, and burn subtitles.

See [CONCEPT.md](CONCEPT.md) for the recipe model and MVP.

## License

MIT
