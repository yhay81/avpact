use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use avpact::error::AvpactError;
use avpact::plan::Recipe;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contracts/v0.1")
}

fn read_json(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn lowercase_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .into_iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 0x0f)]),
            ]
        })
        .collect()
}

fn assert_exact_round_trip<T>(path: &Path, parsed: &T)
where
    T: serde::Serialize,
{
    let original =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let serialized = format!(
        "{}\n",
        serde_json::to_string_pretty(parsed)
            .unwrap_or_else(|error| panic!("serialize {}: {error}", path.display()))
    );
    assert_eq!(
        serialized,
        original,
        "{} is not the exact stable serialization",
        path.display()
    );
}

fn apply_mutation(document: &mut Value, operation: &str, pointer: &str, value: Value) {
    match operation {
        "replace" => {
            let target = document
                .pointer_mut(pointer)
                .unwrap_or_else(|| panic!("replace target {pointer} exists"));
            *target = value;
        }
        "add" => {
            let (parent_pointer, encoded_key) = pointer
                .rsplit_once('/')
                .unwrap_or_else(|| panic!("add pointer {pointer} has a parent"));
            let parent = if parent_pointer.is_empty() {
                &mut *document
            } else {
                document
                    .pointer_mut(parent_pointer)
                    .unwrap_or_else(|| panic!("add parent {parent_pointer} exists"))
            };
            let key = encoded_key.replace("~1", "/").replace("~0", "~");
            parent
                .as_object_mut()
                .unwrap_or_else(|| panic!("add parent {parent_pointer} is an object"))
                .insert(key, value);
        }
        other => panic!("unsupported corpus mutation operation {other}"),
    }
}

#[test]
fn current_readers_accept_and_preserve_v01_documents() {
    let root = corpus_root();
    let manifest = read_json(&root.join("manifest.json"));
    assert_eq!(manifest["schema_version"], "avpact.contract-corpus/v0.1");
    let mut declared_paths = BTreeSet::new();

    for entry in manifest["accepted"]
        .as_array()
        .expect("accepted corpus entries")
    {
        let relative_path = entry["path"].as_str().expect("accepted path");
        assert!(
            declared_paths.insert(relative_path.to_owned()),
            "duplicate accepted fixture {relative_path}"
        );
        let path = root.join(relative_path);
        let bytes = fs::read(&path)
            .unwrap_or_else(|error| panic!("read accepted fixture {}: {error}", path.display()));
        let actual_digest = lowercase_hex(Sha256::digest(&bytes));
        assert_eq!(
            actual_digest,
            entry["sha256"].as_str().expect("accepted SHA-256"),
            "{relative_path} digest changed"
        );
        let value: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("parse accepted fixture {relative_path}: {error}"));
        assert_eq!(
            value["schema_version"], entry["schema_version"],
            "{relative_path} schema version"
        );

        match entry["document"].as_str().expect("accepted document") {
            "recipe" => {
                let recipe: Recipe = serde_json::from_value(value)
                    .unwrap_or_else(|error| panic!("read recipe {relative_path}: {error}"));
                assert_eq!(recipe.schema_version, avpact::RECIPE_SCHEMA_VERSION);
                assert_exact_round_trip(&path, &recipe);
            }
            "plan" => {
                let plan = avpact::plan::read_plan(&path)
                    .unwrap_or_else(|error| panic!("read plan {relative_path}: {error}"));
                avpact::plan::validate_plan(&plan)
                    .unwrap_or_else(|error| panic!("validate plan {relative_path}: {error}"));
                assert_exact_round_trip(&path, &plan);
            }
            "receipt" => {
                let receipt = avpact::apply::read_receipt(&path)
                    .unwrap_or_else(|error| panic!("read receipt {relative_path}: {error}"));
                assert_exact_round_trip(&path, &receipt);
            }
            other => panic!("unsupported accepted document type {other}"),
        }
    }

    let discovered_paths = fs::read_dir(&root)
        .expect("read corpus directory")
        .map(|entry| entry.expect("read corpus entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
                && path.file_name().is_some_and(|name| name != "manifest.json")
        })
        .map(|path| {
            path.file_name()
                .expect("fixture file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        discovered_paths, declared_paths,
        "every accepted JSON fixture must be digest-pinned in the manifest"
    );

    let plan =
        avpact::plan::read_plan(&root.join("plan.clip.json")).expect("read linked golden plan");
    let receipt = avpact::apply::read_receipt(&root.join("receipt.clip.json"))
        .expect("read linked golden receipt");
    assert_eq!(receipt.plan_id, plan.id);
    assert_eq!(
        receipt.plan_digest,
        avpact::plan::plan_digest(&plan).expect("digest linked golden plan")
    );
}

#[test]
fn declared_v01_mutations_fail_closed_with_stable_error_codes() {
    let root = corpus_root();
    let manifest = read_json(&root.join("manifest.json"));
    let mut rejection_ids = BTreeSet::new();

    for case in manifest["rejections"]
        .as_array()
        .expect("rejection corpus entries")
    {
        let id = case["id"].as_str().expect("rejection id");
        assert!(
            rejection_ids.insert(id.to_owned()),
            "duplicate rejection id {id}"
        );
        let document_type = case["document"].as_str().expect("rejection document");
        let base = case["base"].as_str().expect("rejection base");
        let mut document = read_json(&root.join(base));
        apply_mutation(
            &mut document,
            case["operation"].as_str().expect("mutation operation"),
            case["pointer"].as_str().expect("mutation pointer"),
            case["value"].clone(),
        );

        let directory = tempfile::tempdir().expect("mutation directory");
        let path = directory.path().join(format!("{id}.json"));
        fs::write(
            &path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&document).expect("serialize mutation")
            ),
        )
        .expect("write mutation");
        let expected_code = case["expected_error_code"]
            .as_str()
            .expect("expected error code");

        let actual_code = match document_type {
            "recipe" => reject_recipe(&path, directory.path()),
            "plan" => reject_plan(&path),
            "receipt" => avpact::apply::read_receipt(&path)
                .expect_err("mutated receipt must be rejected")
                .code(),
            other => panic!("unsupported rejection document type {other}"),
        };
        assert_eq!(
            actual_code,
            expected_code,
            "rejection {id}: {}",
            case["reason"].as_str().expect("rejection reason")
        );
    }
}

fn reject_recipe(path: &Path, directory: &Path) -> &'static str {
    let output = Command::new(env!("CARGO_BIN_EXE_avpact"))
        .arg("plan")
        .arg(path)
        .arg("--out")
        .arg(directory.join("plan.json"))
        .output()
        .expect("run avpact plan");
    assert!(!output.status.success(), "mutated recipe must be rejected");
    let error: Value = serde_json::from_slice(&output.stderr).unwrap_or_else(|parse_error| {
        panic!(
            "parse recipe rejection: {parse_error}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    match error["error"]["code"].as_str().expect("recipe error code") {
        "recipe_invalid" => "recipe_invalid",
        other => panic!("unexpected recipe error code {other}"),
    }
}

fn reject_plan(path: &Path) -> &'static str {
    match avpact::plan::read_plan(path) {
        Ok(plan) => avpact::plan::validate_plan(&plan)
            .expect_err("mutated plan must be rejected")
            .code(),
        Err(AvpactError::PlanInvalid { .. }) => "plan_invalid",
        Err(error) => panic!("unexpected plan reader error: {error}"),
    }
}
