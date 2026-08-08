use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use avpact::{
    apply::{apply_plan, parse_receipt_document, verify_output},
    plan::{Plan, plan_recipe},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema_version: String,
    license: String,
    generator: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    category: String,
    variant: usize,
    #[serde(default)]
    mutation: Option<Mutation>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mutation {
    pointer: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    code: String,
    destination_changes: usize,
    leaked_temporary_paths: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metrics {
    schema_version: String,
    corpus_sha256: String,
    total_cases: usize,
    detected_cases: usize,
    detection_rate: f64,
    destination_changes: usize,
    leaked_temporary_paths: usize,
    by_category: BTreeMap<String, CategoryMetrics>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryMetrics {
    cases: usize,
    detected_cases: usize,
    detection_rate: f64,
    destination_changes: usize,
    leaked_temporary_paths: usize,
}

#[derive(Debug, Default)]
struct ActualCategory {
    cases: usize,
    detected_cases: usize,
    destination_changes: usize,
    leaked_temporary_paths: usize,
}

struct Outcome {
    code: String,
    destination_changes: usize,
    leaked_temporary_paths: usize,
}

struct MediaFixture {
    _directory: TempDir,
    root: PathBuf,
    source: PathBuf,
    original_source: Vec<u8>,
    plan_path: PathBuf,
    receipt_path: PathBuf,
    plan: Plan,
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversarial/v0.1")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> TestResult<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn backend_available(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn write_clip_recipe(path: &Path) -> TestResult {
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
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
        }))?,
    )?;
    Ok(())
}

fn generate_media(path: &Path) -> TestResult {
    let status = Command::new("ffmpeg")
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
        .arg(path)
        .status()?;
    if !status.success() {
        return Err(io::Error::other("FFmpeg fixture generation failed").into());
    }
    Ok(())
}

fn setup_fixture() -> TestResult<MediaFixture> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join("nested/deeper"))?;
    let source = root.join("source.mp4");
    generate_media(&source)?;
    fs::hard_link(&source, root.join("source-hardlink.mp4"))?;
    let recipe_path = root.join("recipe.json");
    let plan_path = root.join("plan.json");
    let receipt_path = root.join("receipt.json");
    write_clip_recipe(&recipe_path)?;
    let plan = plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))?;
    fs::write(&plan_path, serde_json::to_vec_pretty(&plan)?)?;
    let original_source = fs::read(&source)?;
    Ok(MediaFixture {
        _directory: directory,
        root,
        source,
        original_source,
        plan_path,
        receipt_path,
        plan,
    })
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn reset_apply_paths(fixture: &MediaFixture) -> TestResult {
    remove_if_exists(&fixture.plan.output.path)?;
    remove_if_exists(&fixture.plan.output.temporary_path)?;
    remove_if_exists(&fixture.receipt_path)?;
    Ok(())
}

fn mutated_source(original: &[u8], variant: usize) -> Vec<u8> {
    let mut bytes = original.to_vec();
    match variant {
        0 => bytes.push(b'0'),
        1 => bytes.insert(0, b'1'),
        2 => bytes[0] ^= 0xff,
        3 => bytes.iter_mut().take(16).for_each(|byte| *byte = 0),
        4 => {
            bytes.pop();
        }
        5 => bytes.truncate(bytes.len() / 2),
        6 => bytes.fill(b'X'),
        7 => bytes.extend_from_slice(original),
        8 => bytes.reverse(),
        _ => bytes.rotate_left(17),
    }
    bytes
}

fn run_input_identity_change(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    reset_apply_paths(fixture)?;
    fs::write(
        &fixture.source,
        mutated_source(&fixture.original_source, case.variant),
    )?;
    let result = apply_plan(
        &fixture.plan_path,
        &fixture.receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    );
    fs::write(&fixture.source, &fixture.original_source)?;
    let code = result
        .expect_err("mutated input must fail closed")
        .code()
        .to_owned();
    Ok(Outcome {
        code,
        destination_changes: usize::from(fixture.plan.output.path.exists()),
        leaked_temporary_paths: usize::from(fixture.plan.output.temporary_path.exists()),
    })
}

fn run_receipt_mutation(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    reset_apply_paths(fixture)?;
    let mut receipt: Value =
        serde_json::from_slice(include_bytes!("fixtures/contracts/v0.2/receipt.clip.json"))?;
    let retained_id = receipt["id"].clone();
    let mutation = case
        .mutation
        .as_ref()
        .ok_or_else(|| io::Error::other("receipt mutation metadata missing"))?;
    let target = receipt
        .pointer_mut(&mutation.pointer)
        .ok_or_else(|| io::Error::other("receipt mutation pointer missing"))?;
    *target = mutation.value.clone();
    assert_eq!(
        receipt["id"], retained_id,
        "receipt ID must remain retained"
    );
    let code = parse_receipt_document(&serde_json::to_vec(&receipt)?)
        .expect_err("mutated receipt must fail closed")
        .code()
        .to_owned();
    Ok(Outcome {
        code,
        destination_changes: usize::from(fixture.plan.output.path.exists()),
        leaked_temporary_paths: usize::from(fixture.plan.output.temporary_path.exists()),
    })
}

fn run_output_verification_failure(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    reset_apply_paths(fixture)?;
    fs::write(&fixture.source, &fixture.original_source)?;
    let wrong_output = fixture.root.join("wrong-output.mp4");
    fs::write(&wrong_output, &fixture.original_source)?;
    let mut file = fs::OpenOptions::new().append(true).open(&wrong_output)?;
    file.write_all(&vec![u8::try_from(case.variant + 1)?; case.variant + 1])?;
    drop(file);
    let report = verify_output(
        &wrong_output,
        &fixture.plan,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
    )?;
    let code = if report.passed {
        "verification_unexpectedly_passed"
    } else {
        "verification_failed"
    };
    Ok(Outcome {
        code: code.to_owned(),
        destination_changes: usize::from(fixture.plan.output.path.exists()),
        leaked_temporary_paths: usize::from(fixture.plan.output.temporary_path.exists()),
    })
}

fn alias_paths(case: &Case, fixture: &MediaFixture) -> (String, String) {
    let absolute = fixture.source.to_string_lossy().into_owned();
    match case.variant {
        0 => ("source.mp4".to_owned(), "source.mp4".to_owned()),
        1 => ("./source.mp4".to_owned(), "source.mp4".to_owned()),
        2 => ("source.mp4".to_owned(), "./source.mp4".to_owned()),
        3 => ("nested/../source.mp4".to_owned(), "source.mp4".to_owned()),
        4 => ("source.mp4".to_owned(), "nested/../source.mp4".to_owned()),
        5 => (absolute.clone(), "source.mp4".to_owned()),
        6 => ("source.mp4".to_owned(), absolute),
        7 => (
            "nested/deeper/../../source.mp4".to_owned(),
            "./source.mp4".to_owned(),
        ),
        8 => ("source.mp4".to_owned(), "source-hardlink.mp4".to_owned()),
        _ => ("source-hardlink.mp4".to_owned(), "source.mp4".to_owned()),
    }
}

fn run_unsafe_alias(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    reset_apply_paths(fixture)?;
    fs::write(&fixture.source, &fixture.original_source)?;
    let source_before = fs::read(&fixture.source)?;
    let hardlink_before = fs::read(fixture.root.join("source-hardlink.mp4"))?;
    let (input, output) = alias_paths(case, fixture);
    let recipe_path = fixture.root.join("alias-recipe.json");
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&json!({
            "schema_version": avpact::RECIPE_SCHEMA_VERSION,
            "operation": {
                "type": "clip",
                "input": input,
                "output": output,
                "start_ms": 100,
                "end_ms": 400
            },
            "target": "web",
            "constraints": {
                "overwrite": "deny",
                "duration_tolerance_ms": 100
            }
        }))?,
    )?;
    let code = plan_recipe(&recipe_path, Path::new("ffmpeg"), Path::new("ffprobe"))
        .expect_err("unsafe alias must fail closed")
        .code()
        .to_owned();
    let changed = fs::read(&fixture.source)? != source_before
        || fs::read(fixture.root.join("source-hardlink.mp4"))? != hardlink_before;
    Ok(Outcome {
        code,
        destination_changes: usize::from(changed),
        leaked_temporary_paths: usize::from(fixture.plan.output.temporary_path.exists()),
    })
}

fn run_existing_destination(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    reset_apply_paths(fixture)?;
    fs::write(&fixture.source, &fixture.original_source)?;
    let sentinel = vec![b'A' + u8::try_from(case.variant)?; 32 + case.variant];
    fs::write(&fixture.plan.output.path, &sentinel)?;
    let result = apply_plan(
        &fixture.plan_path,
        &fixture.receipt_path,
        Path::new("ffmpeg"),
        Path::new("ffprobe"),
        |_| {},
    );
    let code = result
        .expect_err("existing destination must fail closed")
        .code()
        .to_owned();
    Ok(Outcome {
        code,
        destination_changes: usize::from(fs::read(&fixture.plan.output.path)? != sentinel),
        leaked_temporary_paths: usize::from(fixture.plan.output.temporary_path.exists()),
    })
}

fn execute_case(case: &Case, fixture: &MediaFixture) -> TestResult<Outcome> {
    match case.category.as_str() {
        "input_identity_change" => run_input_identity_change(case, fixture),
        "receipt_mutation" => run_receipt_mutation(case, fixture),
        "output_verification_failure" => run_output_verification_failure(case, fixture),
        "unsafe_alias" => run_unsafe_alias(case, fixture),
        "existing_destination" => run_existing_destination(case, fixture),
        other => Err(io::Error::other(format!("unknown corpus category {other}")).into()),
    }
}

fn ratio(numerator: usize, denominator: usize) -> TestResult<f64> {
    if denominator == 0 {
        return Ok(0.0);
    }
    Ok(f64::from(u32::try_from(numerator)?) / f64::from(u32::try_from(denominator)?))
}

fn approximately_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= f64::EPSILON
}

fn canonical_sha256(value: &Value) -> TestResult<String> {
    let digest = Sha256::digest(serde_json::to_vec(value)?);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[test]
fn published_adversarial_metrics_are_reproducible() -> TestResult {
    let root = corpus_root();
    let corpus_path = root.join("corpus.json");
    let corpus: Corpus = read_json(&corpus_path)?;
    let metrics: Metrics = read_json(&root.join("metrics.json"))?;
    let corpus_value: Value = serde_json::from_slice(&fs::read(&corpus_path)?)?;
    assert_eq!(corpus.schema_version, "avpact.adversarial-corpus/v0.1");
    assert_eq!(corpus.license, "MIT");
    assert_eq!(corpus.generator, "generate_corpus.py");
    assert_eq!(metrics.schema_version, "avpact.adversarial-metrics/v0.1");
    assert_eq!(metrics.corpus_sha256, canonical_sha256(&corpus_value)?);

    if !backend_available("ffmpeg") || !backend_available("ffprobe") {
        eprintln!("skipping media-backed adversarial scoring: FFmpeg or FFprobe is unavailable");
        return Ok(());
    }

    let fixture = setup_fixture()?;
    let mut ids = BTreeSet::new();
    let mut by_category = BTreeMap::<String, ActualCategory>::new();
    let mut detected_cases = 0;
    let mut destination_changes = 0;
    let mut leaked_temporary_paths = 0;
    for case in &corpus.cases {
        assert!(ids.insert(case.id.as_str()), "duplicate case {}", case.id);
        let outcome = execute_case(case, &fixture)?;
        let detected = outcome.code == case.expected.code;
        assert!(
            detected,
            "signal mismatch for {}: {}",
            case.id, outcome.code
        );
        assert_eq!(
            outcome.destination_changes, case.expected.destination_changes,
            "destination mutation mismatch for {}",
            case.id
        );
        assert_eq!(
            outcome.leaked_temporary_paths, case.expected.leaked_temporary_paths,
            "temporary leak mismatch for {}",
            case.id
        );

        detected_cases += usize::from(detected);
        destination_changes += outcome.destination_changes;
        leaked_temporary_paths += outcome.leaked_temporary_paths;
        let category = by_category.entry(case.category.clone()).or_default();
        category.cases += 1;
        category.detected_cases += usize::from(detected);
        category.destination_changes += outcome.destination_changes;
        category.leaked_temporary_paths += outcome.leaked_temporary_paths;
    }

    assert_eq!(metrics.total_cases, corpus.cases.len());
    assert_eq!(metrics.detected_cases, detected_cases);
    assert_eq!(metrics.destination_changes, destination_changes);
    assert_eq!(metrics.leaked_temporary_paths, leaked_temporary_paths);
    assert!(approximately_equal(
        metrics.detection_rate,
        ratio(detected_cases, corpus.cases.len())?
    ));
    assert_eq!(metrics.by_category.len(), by_category.len());
    for (name, actual) in &by_category {
        let expected = metrics
            .by_category
            .get(name)
            .ok_or_else(|| io::Error::other(format!("missing metrics for {name}")))?;
        assert_eq!(expected.cases, actual.cases);
        assert_eq!(expected.detected_cases, actual.detected_cases);
        assert_eq!(expected.destination_changes, actual.destination_changes);
        assert_eq!(
            expected.leaked_temporary_paths,
            actual.leaked_temporary_paths
        );
        assert!(approximately_equal(
            expected.detection_rate,
            ratio(actual.detected_cases, actual.cases)?
        ));
    }
    Ok(())
}
