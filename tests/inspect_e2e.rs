use std::path::Path;
use std::process::Command;

use avpact::model::StreamKind;
use serde_json::json;

#[test]
fn inspects_generated_audio_video_media_when_ffmpeg_is_available() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping generated-media test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("sample.mp4");
    generate_media(&media);

    let report =
        avpact::inspect::inspect(&media, Path::new("ffprobe")).expect("inspect generated media");

    assert_eq!(report.format.duration_ms, Some(500));
    assert!(report.source.size_bytes > 0);
    assert_eq!(report.source.sha256.len(), 64);
    assert!(
        report
            .streams
            .iter()
            .any(|stream| stream.kind == StreamKind::Video)
    );
    assert!(
        report
            .streams
            .iter()
            .any(|stream| stream.kind == StreamKind::Audio)
    );
}

#[test]
fn inspects_video_only_audio_only_and_still_image_fixtures() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping diverse inspection test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    let video = directory.path().join("video-only.mp4");
    let audio = directory.path().join("audio-only.wav");
    let image = directory.path().join("still.jpg");
    generate_with_args(
        &video,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=128x72:rate=12",
            "-t",
            "0.3",
            "-an",
            "-c:v",
            "mpeg4",
        ],
    );
    generate_with_args(
        &audio,
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "0.3",
            "-vn",
            "-c:a",
            "pcm_s16le",
        ],
    );
    generate_with_args(
        &image,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=96x54:rate=1",
            "-frames:v",
            "1",
            "-c:v",
            "mjpeg",
        ],
    );

    let video_report =
        avpact::inspect::inspect(&video, Path::new("ffprobe")).expect("inspect video-only");
    let audio_report =
        avpact::inspect::inspect(&audio, Path::new("ffprobe")).expect("inspect audio-only");
    let image_report =
        avpact::inspect::inspect(&image, Path::new("ffprobe")).expect("inspect still image");

    assert!(
        video_report
            .streams
            .iter()
            .any(|stream| stream.kind == StreamKind::Video)
    );
    assert!(
        video_report
            .streams
            .iter()
            .all(|stream| stream.kind != StreamKind::Audio)
    );
    assert!(
        audio_report
            .streams
            .iter()
            .any(|stream| stream.kind == StreamKind::Audio)
    );
    assert!(
        audio_report
            .streams
            .iter()
            .all(|stream| stream.kind != StreamKind::Video)
    );
    assert_eq!(
        image_report
            .streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Video)
            .and_then(|stream| stream.video.as_ref())
            .and_then(|video| video.width.zip(video.height)),
        Some((96, 54))
    );
}

#[test]
fn plans_a_clip_without_creating_the_media_output() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping clip planning test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    generate_media(&media);
    let recipe_path = directory.path().join("recipe.json");
    let output = directory.path().join("clip.mp4");
    write_clip_recipe(&recipe_path);

    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan clip");

    assert!(!output.exists(), "planning must not create media output");
    assert_eq!(plan.schema_version, avpact::PLAN_SCHEMA_VERSION);
    assert_eq!(plan.expected.duration_ms, Some(300));
    assert_eq!(
        plan.expected
            .video
            .as_ref()
            .map(|video| video.codec.as_str()),
        Some("h264")
    );
    assert!(
        plan.backend
            .argv
            .iter()
            .any(|argument| argument == "libx264")
    );
}

#[test]
fn applies_verifies_and_receipts_a_planned_clip() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping apply test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    generate_media(&media);
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    let output = directory.path().join("clip.mp4");
    write_clip_recipe(&recipe_path);
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan clip");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");
    let mut progress = Vec::new();

    let receipt = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |event| progress.push(event.clone()),
    )
    .expect("apply clip");

    assert!(output.is_file());
    assert!(!plan.output.temporary_path.exists());
    assert!(receipt_path.is_file());
    assert!(receipt.verification.passed);
    assert_eq!(receipt.verification.output.source.path, plan.output.path);
    assert!(
        progress
            .iter()
            .any(|event| matches!(event.state, avpact::apply::ProgressState::Finished))
    );

    let verification =
        avpact::apply::verify_output(&output, &plan, Path::new("ffmpeg"), Path::new("ffprobe"))
            .expect("verify published output");
    assert!(verification.passed);
}

#[test]
fn rolls_back_publication_when_receipt_persistence_fails() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping receipt rollback test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    generate_media(&media);
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let output = directory.path().join("clip.mp4");
    let receipt_path = directory
        .path()
        .join(format!("{}.json", "receipt".repeat(64)));
    write_clip_recipe(&recipe_path);
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan clip");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");

    let result = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    );

    assert!(matches!(
        result,
        Err(avpact::error::AvpactError::ReceiptWrite { .. })
    ));
    assert!(
        !output.exists(),
        "receipt failure must roll back the published output"
    );
    assert!(!plan.output.temporary_path.exists());
    assert!(!receipt_path.exists());
}

#[test]
fn refuses_input_drift_without_publishing_output() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping input drift test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    generate_media(&media);
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    let output = directory.path().join("clip.mp4");
    write_clip_recipe(&recipe_path);
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan clip");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&media)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"changed"))
        .expect("change planned input");

    let result = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    );

    assert!(matches!(
        result,
        Err(avpact::error::AvpactError::InputChanged { .. })
    ));
    assert!(!output.exists());
    assert!(!receipt_path.exists());
}

#[test]
fn refuses_an_output_created_after_planning() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping no-clobber test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    generate_media(&media);
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    let output = directory.path().join("clip.mp4");
    write_clip_recipe(&recipe_path);
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan clip");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");
    std::fs::write(&output, b"do not replace").expect("create racing output");

    let result = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    );

    assert!(matches!(
        result,
        Err(avpact::error::AvpactError::OutputExists { .. })
    ));
    assert_eq!(
        std::fs::read(&output).expect("read protected output"),
        b"do not replace"
    );
    assert!(!receipt_path.exists());
}

#[test]
fn refuses_a_recipe_whose_estimate_exceeds_its_resource_budget() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping resource budget test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    generate_media(&directory.path().join("source.mp4"));
    let recipe_path = directory.path().join("recipe.json");
    let recipe = json!({
        "schema_version": avpact::RECIPE_SCHEMA_VERSION,
        "operation": {
            "type": "transcode",
            "input": "source.mp4",
            "output": "output.mp4"
        },
        "constraints": {
            "max_output_bytes": 1024,
            "max_temporary_bytes": 2048,
            "max_runtime_ms": 1000
        }
    });
    std::fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&recipe).expect("serialize recipe"),
    )
    .expect("write recipe");

    let result = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"));

    assert!(matches!(
        result,
        Err(avpact::error::AvpactError::RecipeInvalid { .. })
    ));
    assert!(!directory.path().join("output.mp4").exists());
}

#[test]
fn transcodes_to_the_web_target_end_to_end() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping transcode test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let (directory, plan, receipt) = apply_operation(json!({
        "type": "transcode",
        "input": "source.mp4",
        "output": "transcoded.mp4"
    }));

    assert!(directory.path().join("transcoded.mp4").is_file());
    assert!(receipt.verification.passed);
    assert_eq!(
        plan.expected
            .video
            .as_ref()
            .map(|video| (video.width, video.height)),
        Some((160, 90))
    );
}

#[test]
fn applies_all_resize_modes_and_rotation() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping resize tests: ffmpeg or ffprobe is unavailable");
        return;
    }
    for (mode, rotation, expected) in [
        ("stretch", "none", (120, 120)),
        ("fit", "none", (120, 66)),
        ("crop", "none", (120, 120)),
        ("pad", "none", (120, 120)),
        ("fit", "clockwise90", (66, 120)),
    ] {
        let output = format!("{mode}-{rotation}.mp4");
        let (directory, plan, receipt) = apply_operation(json!({
            "type": "resize",
            "input": "source.mp4",
            "output": output.clone(),
            "width": 120,
            "height": 120,
            "mode": mode,
            "rotation": rotation
        }));

        assert!(directory.path().join(&output).is_file());
        assert!(receipt.verification.passed);
        assert_eq!(
            plan.expected
                .video
                .as_ref()
                .map(|video| (video.width, video.height)),
            Some(expected),
            "mode={mode} rotation={rotation}"
        );
    }
}

#[test]
fn extracts_audio_without_a_video_stream() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping audio extraction test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let (directory, plan, receipt) = apply_operation(json!({
        "type": "extract_audio",
        "input": "source.mp4",
        "output": "audio.m4a"
    }));

    assert!(directory.path().join("audio.m4a").is_file());
    assert!(receipt.verification.passed);
    assert!(plan.expected.video.is_none());
    assert!(plan.expected.audio.is_some());
    assert!(
        receipt
            .verification
            .output
            .streams
            .iter()
            .all(|stream| stream.kind != StreamKind::Video)
    );
}

#[test]
fn normalizes_and_measures_integrated_loudness() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping loudness normalization test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let (directory, plan, receipt) = apply_operation(json!({
        "type": "normalize_audio",
        "input": "source.mp4",
        "output": "normalized.m4a",
        "target_lufs": -14,
        "tolerance_lu_x100": 150
    }));

    assert!(directory.path().join("normalized.m4a").is_file());
    assert!(receipt.verification.passed);
    assert!(
        receipt
            .verification
            .checks
            .iter()
            .any(|check| check.check == "integrated_loudness" && check.passed)
    );
    assert!(
        plan.expected
            .audio
            .as_ref()
            .and_then(|audio| audio.loudness.as_ref())
            .is_some()
    );
}

#[test]
fn concatenates_multiple_inputs_with_explicit_streams() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping concatenate test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    generate_media(&directory.path().join("source-a.mp4"));
    generate_media(&directory.path().join("source-b.mp4"));
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    let output = directory.path().join("joined.mp4");
    let recipe = json!({
        "schema_version": avpact::RECIPE_SCHEMA_VERSION,
        "operation": {
            "type": "concatenate",
            "inputs": ["source-a.mp4", "source-b.mp4"],
            "output": "joined.mp4"
        },
        "constraints": {
            "overwrite": "deny",
            "duration_tolerance_ms": 100
        }
    });
    std::fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&recipe).expect("serialize recipe"),
    )
    .expect("write recipe");
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan concatenate");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");

    let receipt = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    )
    .expect("apply concatenate");

    assert!(output.is_file());
    assert_eq!(plan.inputs.len(), 2);
    assert_eq!(plan.expected.duration_ms, Some(1_000));
    assert!(receipt.verification.passed);
}

#[test]
fn rejects_incompatible_concatenation_dimensions() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping concatenate compatibility test: backend unavailable");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    generate_media(&directory.path().join("source-a.mp4"));
    generate_with_args(
        &directory.path().join("source-b.mp4"),
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=120x120:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-t",
            "0.5",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
        ],
    );
    let recipe_path = directory.path().join("recipe.json");
    std::fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": avpact::RECIPE_SCHEMA_VERSION,
            "operation": {
                "type": "concatenate",
                "inputs": ["source-a.mp4", "source-b.mp4"],
                "output": "joined.mp4"
            }
        }))
        .expect("serialize recipe"),
    )
    .expect("write recipe");

    let result = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"));

    assert!(matches!(
        result,
        Err(avpact::error::AvpactError::Unsupported { .. })
    ));
}

#[test]
fn creates_a_verified_thumbnail_and_contact_sheet() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping image output tests: ffmpeg or ffprobe is unavailable");
        return;
    }
    let (thumbnail_dir, thumbnail_plan, thumbnail_receipt) = apply_operation(json!({
        "type": "thumbnail",
        "input": "source.mp4",
        "output": "thumbnail.jpg",
        "at_ms": 100,
        "width": 120
    }));
    assert!(thumbnail_dir.path().join("thumbnail.jpg").is_file());
    assert!(thumbnail_receipt.verification.passed);
    assert_eq!(
        thumbnail_plan
            .expected
            .video
            .as_ref()
            .map(|video| (video.width, video.height)),
        Some((120, 66))
    );

    let (sheet_dir, sheet_plan, sheet_receipt) = apply_operation(json!({
        "type": "contact_sheet",
        "input": "source.mp4",
        "output": "sheet.jpg",
        "interval_ms": 100,
        "columns": 2,
        "rows": 2,
        "width": 240
    }));
    assert!(sheet_dir.path().join("sheet.jpg").is_file());
    assert!(sheet_receipt.verification.passed);
    assert_eq!(
        sheet_plan
            .expected
            .video
            .as_ref()
            .map(|video| (video.width, video.height)),
        Some((240, 132))
    );
}

#[test]
fn burns_subtitles_into_verified_video_output() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping subtitle burn-in test: ffmpeg or ffprobe is unavailable");
        return;
    }
    if !has_filter("subtitles") {
        eprintln!("skipping subtitle burn-in test: FFmpeg lacks the subtitles filter");
        return;
    }
    let directory = tempfile::tempdir().expect("temporary directory");
    generate_media(&directory.path().join("source.mp4"));
    std::fs::write(
        directory.path().join("captions.srt"),
        include_str!("fixtures/media/subtitle.srt"),
    )
    .expect("write subtitle fixture");
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    let output = directory.path().join("subtitled.mp4");
    let recipe = json!({
        "schema_version": avpact::RECIPE_SCHEMA_VERSION,
        "operation": {
            "type": "burn_subtitles",
            "input": "source.mp4",
            "subtitles": "captions.srt",
            "output": "subtitled.mp4"
        }
    });
    std::fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&recipe).expect("serialize recipe"),
    )
    .expect("write recipe");
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan subtitle burn-in");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");

    let receipt = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    )
    .expect("apply subtitle burn-in");

    assert!(output.is_file());
    assert_eq!(plan.inputs.len(), 2);
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.code == "subtitles_burned")
    );
    assert!(receipt.verification.passed);
    assert!(
        receipt
            .verification
            .output
            .streams
            .iter()
            .all(|stream| stream.kind != StreamKind::Subtitle)
    );
}

fn apply_operation(
    operation: serde_json::Value,
) -> (
    tempfile::TempDir,
    avpact::plan::Plan,
    avpact::apply::Receipt,
) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let media = directory.path().join("source.mp4");
    let recipe_path = directory.path().join("recipe.json");
    let plan_path = directory.path().join("plan.json");
    let receipt_path = directory.path().join("receipt.json");
    generate_media(&media);
    let recipe = json!({
        "schema_version": avpact::RECIPE_SCHEMA_VERSION,
        "operation": operation,
        "constraints": {
            "overwrite": "deny",
            "duration_tolerance_ms": 100
        }
    });
    std::fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&recipe).expect("serialize recipe"),
    )
    .expect("write recipe");
    let plan = avpact::plan::plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect("plan operation");
    std::fs::write(
        &plan_path,
        serde_json::to_vec_pretty(&plan).expect("serialize plan"),
    )
    .expect("write plan");
    let receipt = avpact::apply::apply_plan(
        &plan_path,
        &receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    )
    .expect("apply operation");
    (directory, plan, receipt)
}

fn write_clip_recipe(recipe_path: &Path) {
    let recipe = json!({
        "schema_version": avpact::RECIPE_SCHEMA_VERSION,
        "operation": {
            "type": "clip",
            "input": "source.mp4",
            "output": "clip.mp4",
            "start_ms": 100,
            "end_ms": 400
        },
        "target": "web",
        "constraints": {
            "overwrite": "deny",
            "duration_tolerance_ms": 100
        }
    });
    std::fs::write(
        recipe_path,
        serde_json::to_vec_pretty(&recipe).expect("serialize recipe"),
    )
    .expect("write recipe");
}

fn generate_media(media: &Path) {
    let generated = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=10",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:sample_rate=48000",
            "-t",
            "0.5",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
        ])
        .arg(media)
        .status()
        .expect("start ffmpeg");
    assert!(generated.success(), "generate media fixture");
}

fn generate_with_args(media: &Path, arguments: &[&str]) {
    let generated = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error"])
        .args(arguments)
        .arg(media)
        .status()
        .expect("start ffmpeg fixture generation");
    assert!(generated.success(), "generate {}", media.display());
}

fn is_available(executable: &str) -> bool {
    Command::new(executable)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn has_filter(filter: &str) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.split_whitespace()
                    .nth(1)
                    .is_some_and(|candidate| candidate == filter)
            })
        })
}
