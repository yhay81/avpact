use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

#[test]
fn cli_reports_backend_capabilities() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping capability test: ffmpeg or ffprobe is unavailable");
        return;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .args(["capabilities", "--format", "json"])
        .output()
        .expect("run capabilities");
    assert!(
        output.status.success(),
        "capabilities stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("capability JSON");
    assert_eq!(report["schema_version"], avpact::CAPABILITY_SCHEMA_VERSION);
    assert!(
        report["operations"]
            .as_array()
            .is_some_and(|operations| operations.len() >= 9)
    );
}

#[test]
fn cli_runs_the_clip_workflow_with_json_contracts() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping CLI workflow test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("source.mp4");
    let output = directory.path().join("clip.mp4");
    let recipe = directory.path().join("recipe.json");
    let plan = directory.path().join("plan.json");
    let state_dir = directory.path().join(".avpact");
    generate_media(&source);
    std::fs::write(
        &recipe,
        serde_json::to_vec_pretty(&json!({
            "schema_version": avpact::RECIPE_SCHEMA_VERSION,
            "operation": {
                "type": "clip",
                "input": "source.mp4",
                "output": "clip.mp4",
                "start_ms": 100,
                "end_ms": 400
            }
        }))
        .expect("serialize recipe"),
    )
    .expect("write recipe");

    let planned = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .args(["plan"])
        .arg(&recipe)
        .args(["--out"])
        .arg(&plan)
        .args(["--format", "json"])
        .output()
        .expect("run plan command");
    assert!(
        planned.status.success(),
        "plan stderr: {}",
        String::from_utf8_lossy(&planned.stderr)
    );
    let plan_stdout: Value = serde_json::from_slice(&planned.stdout).expect("plan stdout JSON");
    assert_eq!(plan_stdout["schema_version"], avpact::PLAN_SCHEMA_VERSION);
    assert!(plan.is_file());
    assert!(!output.exists());

    let applied = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .arg("apply")
        .arg(&plan)
        .args(["--progress", "ndjson", "--format", "json"])
        .output()
        .expect("run apply command");
    assert!(
        applied.status.success(),
        "apply stderr: {}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let receipt_stdout: Value =
        serde_json::from_slice(&applied.stdout).expect("receipt stdout JSON");
    assert_eq!(
        receipt_stdout["schema_version"],
        avpact::RECEIPT_SCHEMA_VERSION
    );
    for line in String::from_utf8_lossy(&applied.stderr).lines() {
        let event: Value = serde_json::from_str(line).expect("progress NDJSON");
        assert_eq!(event["schema_version"], avpact::PROGRESS_SCHEMA_VERSION);
    }
    assert!(output.is_file());
    let receipt_id = receipt_stdout["id"].as_str().expect("receipt id");
    let stored_receipt = state_dir
        .join("receipts")
        .join(format!("{receipt_id}.json"));
    assert!(stored_receipt.is_file());

    let shown = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .args(["receipt", "show", receipt_id, "--state-dir"])
        .arg(&state_dir)
        .args(["--format", "json"])
        .output()
        .expect("show stored receipt");
    assert!(
        shown.status.success(),
        "receipt show stderr: {}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let shown_receipt: Value = serde_json::from_slice(&shown.stdout).expect("shown receipt JSON");
    assert_eq!(shown_receipt, receipt_stdout);

    let verified = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .arg("verify")
        .arg(&output)
        .arg("--against")
        .arg(&plan)
        .args(["--format", "json"])
        .output()
        .expect("run verify command");
    assert!(
        verified.status.success(),
        "verify stderr: {}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let verification: Value =
        serde_json::from_slice(&verified.stdout).expect("verification stdout JSON");
    assert_eq!(
        verification["schema_version"],
        avpact::VERIFICATION_SCHEMA_VERSION
    );
    assert_eq!(verification["passed"], true);
}

#[test]
fn cli_emits_machine_actionable_receipt_recovery_error() {
    if !is_available("ffmpeg") || !is_available("ffprobe") {
        eprintln!("skipping CLI receipt recovery test: ffmpeg or ffprobe is unavailable");
        return;
    }

    let directory = tempfile::tempdir().expect("temporary directory");
    let source = directory.path().join("source.mp4");
    let output = directory.path().join("clip.mp4");
    let recipe = directory.path().join("recipe.json");
    let plan = directory.path().join("plan.json");
    let receipt = directory
        .path()
        .join(format!("{}.json", "receipt".repeat(64)));
    generate_media(&source);
    std::fs::write(
        &recipe,
        serde_json::to_vec_pretty(&json!({
            "schema_version": avpact::RECIPE_SCHEMA_VERSION,
            "operation": {
                "type": "clip",
                "input": "source.mp4",
                "output": "clip.mp4",
                "start_ms": 100,
                "end_ms": 400
            }
        }))
        .expect("serialize recipe"),
    )
    .expect("write recipe");

    let planned = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .arg("plan")
        .arg(&recipe)
        .arg("--out")
        .arg(&plan)
        .args(["--format", "json"])
        .output()
        .expect("run plan command");
    assert!(
        planned.status.success(),
        "plan stderr: {}",
        String::from_utf8_lossy(&planned.stderr)
    );

    let applied = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .arg("apply")
        .arg(&plan)
        .arg("--receipt-out")
        .arg(&receipt)
        .args(["--progress", "ndjson", "--format", "json"])
        .output()
        .expect("run apply command");
    assert!(!applied.status.success());
    assert!(applied.stdout.is_empty());
    assert!(output.is_file());

    let error: Value = String::from_utf8_lossy(&applied.stderr)
        .lines()
        .last()
        .map(serde_json::from_str)
        .expect("error line")
        .expect("error JSON");
    assert_eq!(error["schema_version"], avpact::ERROR_SCHEMA_VERSION);
    assert_eq!(error["error"]["code"], "receipt_recovery_required");
    assert_eq!(error["error"]["recovery"]["action"], "do_not_retry_apply");
    assert_eq!(
        error["error"]["recovery"]["recovery_receipt_persisted"],
        true
    );
    assert_eq!(
        error["error"]["recovery"]["output_sha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    let recovery_receipt = PathBuf::from(
        error["error"]["recovery"]["recovery_receipt"]
            .as_str()
            .expect("recovery receipt path"),
    );
    assert!(recovery_receipt.is_file());
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

fn is_available(executable: &str) -> bool {
    Command::new(executable)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}
