use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AvpactError, bounded_diagnostic};
use crate::hex::encode_lower;
use crate::inspect;
use crate::model::{InspectionReport, SourceIdentity, StreamKind, StreamSummary};

const MAX_PLAN_WARNINGS: usize = 128;
const MAX_WARNING_CHARACTERS: usize = 2_048;
const MAX_PLAN_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECIPE_DOCUMENT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub schema_version: String,
    pub operation: Operation,
    #[serde(default)]
    pub target: Target,
    #[serde(default)]
    pub constraints: RecipeConstraints,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Clip {
        input: PathBuf,
        output: PathBuf,
        start_ms: u64,
        end_ms: u64,
    },
    Transcode {
        input: PathBuf,
        output: PathBuf,
    },
    Resize {
        input: PathBuf,
        output: PathBuf,
        width: u32,
        height: u32,
        mode: ResizeMode,
        #[serde(default)]
        rotation: Rotation,
    },
    ExtractAudio {
        input: PathBuf,
        output: PathBuf,
    },
    NormalizeAudio {
        input: PathBuf,
        output: PathBuf,
        #[serde(default = "default_target_lufs")]
        target_lufs: i32,
        #[serde(default = "default_loudness_tolerance")]
        tolerance_lu_x100: u32,
    },
    Concatenate {
        inputs: Vec<PathBuf>,
        output: PathBuf,
    },
    Thumbnail {
        input: PathBuf,
        output: PathBuf,
        at_ms: u64,
        width: u32,
    },
    ContactSheet {
        input: PathBuf,
        output: PathBuf,
        interval_ms: u64,
        columns: u32,
        rows: u32,
        width: u32,
    },
    BurnSubtitles {
        input: PathBuf,
        subtitles: PathBuf,
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResizeMode {
    Stretch,
    Fit,
    Crop,
    Pad,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    CounterClockwise90,
}

fn default_target_lufs() -> i32 {
    -14
}

fn default_loudness_tolerance() -> u32 {
    100
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[default]
    Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RecipeConstraints {
    pub overwrite: OverwritePolicy,
    pub duration_tolerance_ms: u64,
    pub max_output_bytes: u64,
    pub max_temporary_bytes: u64,
    pub max_runtime_ms: u64,
}

impl Default for RecipeConstraints {
    fn default() -> Self {
        Self {
            overwrite: OverwritePolicy::Deny,
            duration_tolerance_ms: 100,
            max_output_bytes: 10 * 1024 * 1024 * 1024,
            max_temporary_bytes: 12 * 1024 * 1024 * 1024,
            max_runtime_ms: 4 * 60 * 60 * 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    #[default]
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: String,
    pub id: String,
    pub recipe_digest: String,
    pub constraints_digest: String,
    pub inputs: Vec<InspectionReport>,
    pub output: PlannedOutput,
    pub operation: PlannedOperation,
    pub selected_streams: Vec<SelectedStream>,
    pub backend: PlannedBackend,
    pub expected: ExpectedOutput,
    pub verification_checks: Vec<VerificationCheck>,
    pub warnings: Vec<PlanWarning>,
    pub resources: ResourcePlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannedOutput {
    pub path: PathBuf,
    pub temporary_path: PathBuf,
    pub overwrite: OverwritePolicy,
    pub atomic_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedOperation {
    Clip {
        start_ms: u64,
        end_ms: u64,
        duration_ms: u64,
        accuracy: ClipAccuracy,
    },
    Transcode,
    Resize {
        requested_width: u32,
        requested_height: u32,
        output_width: u32,
        output_height: u32,
        mode: ResizeMode,
        rotation: Rotation,
    },
    ExtractAudio,
    NormalizeAudio {
        target_lufs_x100: i32,
        tolerance_lu_x100: u32,
    },
    Concatenate {
        segment_count: usize,
        duration_ms: Option<u64>,
    },
    Thumbnail {
        at_ms: u64,
        width: u32,
        height: u32,
    },
    ContactSheet {
        interval_ms: u64,
        columns: u32,
        rows: u32,
        width: u32,
        height: u32,
    },
    BurnSubtitles,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipAccuracy {
    ExactTranscode,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectedStream {
    pub source_index: usize,
    pub stream_index: u32,
    pub kind: StreamKind,
    pub reason: String,
    pub output_codec: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlannedBackend {
    pub name: String,
    pub version: String,
    pub configuration: String,
    pub library_versions: BTreeMap<String, String>,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpectedOutput {
    pub container: String,
    pub duration_ms: Option<u64>,
    pub duration_tolerance_ms: u64,
    pub video: Option<ExpectedVideo>,
    pub audio: Option<ExpectedAudio>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpectedVideo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub sample_aspect_ratio: String,
    pub average_frame_rate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpectedAudio {
    pub codec: String,
    pub channels: Option<u32>,
    pub loudness: Option<ExpectedLoudness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExpectedLoudness {
    pub integrated_lufs_x100: i32,
    pub tolerance_lu_x100: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerificationCheck {
    Parseable,
    Container {
        name: String,
    },
    Duration {
        expected_ms: u64,
        tolerance_ms: u64,
    },
    Video {
        codec: String,
        width: u32,
        height: u32,
        sample_aspect_ratio: String,
        average_frame_rate: Option<String>,
    },
    StreamLayout {
        video_streams: u32,
        audio_streams: u32,
        subtitle_streams: u32,
        other_streams: u32,
    },
    Audio {
        codec: String,
        channels: Option<u32>,
    },
    Loudness {
        integrated_lufs_x100: i32,
        tolerance_lu_x100: u32,
    },
    OutputSize {
        max_bytes: u64,
    },
    DistinctFromInputs,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourcePlan {
    pub class: ResourceClass,
    pub estimated_output_bytes: u64,
    pub estimated_temporary_bytes: u64,
    pub max_output_bytes: u64,
    pub max_temporary_bytes: u64,
    pub max_runtime_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    Light,
    Standard,
    Heavy,
}

struct PlanInputs {
    recipe: Recipe,
    recipe_digest: String,
    constraints_digest: String,
    inputs: Vec<InspectionReport>,
    output: PathBuf,
    backend: FfmpegBuildIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FfmpegBuildIdentity {
    pub version: String,
    pub configuration: String,
    pub library_versions: BTreeMap<String, String>,
}

pub fn plan_recipe(recipe_path: &Path, ffmpeg: &Path, ffprobe: &Path) -> Result<Plan, AvpactError> {
    let bytes =
        read_bounded_document(recipe_path, MAX_RECIPE_DOCUMENT_BYTES).map_err(|source| {
            AvpactError::RecipeRead {
                path: recipe_path.to_path_buf(),
                source,
            }
        })?;
    if bytes.len() as u64 > MAX_RECIPE_DOCUMENT_BYTES {
        return Err(AvpactError::RecipeInvalid {
            message: format!(
                "recipe exceeds the {} byte document limit",
                MAX_RECIPE_DOCUMENT_BYTES
            ),
        });
    }
    let recipe: Recipe =
        serde_json::from_slice(&bytes).map_err(|source| AvpactError::RecipeInvalid {
            message: source.to_string(),
        })?;
    validate_recipe(&recipe)?;

    let recipe_directory = recipe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|source| AvpactError::RecipeRead {
            path: recipe_path.to_path_buf(),
            source,
        })?;
    let (declared_inputs, declared_output, output_extension, uses_h264) = match &recipe.operation {
        Operation::Clip { input, output, .. }
        | Operation::Transcode { input, output }
        | Operation::Resize { input, output, .. } => (vec![input], output, "mp4", true),
        Operation::ExtractAudio { input, output }
        | Operation::NormalizeAudio { input, output, .. } => (vec![input], output, "m4a", false),
        Operation::Concatenate { inputs, output } => (inputs.iter().collect(), output, "mp4", true),
        Operation::Thumbnail { input, output, .. }
        | Operation::ContactSheet { input, output, .. } => (vec![input], output, "jpg", false),
        Operation::BurnSubtitles {
            input,
            subtitles,
            output,
        } => (vec![input, subtitles], output, "mp4", true),
    };
    let needs_audio_encoder = matches!(
        &recipe.operation,
        Operation::ExtractAudio { .. } | Operation::NormalizeAudio { .. }
    );
    let mut inspected_inputs = Vec::with_capacity(declared_inputs.len());
    for declared_input in declared_inputs {
        let input_path = resolve_input(&recipe_directory, declared_input);
        inspected_inputs.push(inspect::inspect(&input_path, ffprobe)?);
    }
    let output = resolve_output(&recipe_directory, declared_output)?;
    for input in &inspected_inputs {
        validate_paths(&input.source, &output, recipe.constraints.overwrite)?;
    }

    if output.extension() != Some(OsStr::new(output_extension)) {
        return Err(AvpactError::Unsupported {
            message: format!("this operation requires a .{output_extension} output"),
        });
    }

    let backend = backend_identity(ffmpeg)?;
    let preserves_audio = matches!(
        &recipe.operation,
        Operation::Clip { .. }
            | Operation::Transcode { .. }
            | Operation::Resize { .. }
            | Operation::Concatenate { .. }
            | Operation::BurnSubtitles { .. }
    );
    let has_audio = inspected_inputs
        .iter()
        .any(|input| input.streams.iter().any(is_audio));
    let mut required_encoders = Vec::new();
    if uses_h264 {
        required_encoders.push("libx264");
    }
    if needs_audio_encoder || (preserves_audio && has_audio) {
        required_encoders.push("aac");
    }
    if matches!(
        &recipe.operation,
        Operation::Thumbnail { .. } | Operation::ContactSheet { .. }
    ) {
        required_encoders.push("mjpeg");
    }
    validate_encoders(ffmpeg, &required_encoders)?;
    if matches!(&recipe.operation, Operation::NormalizeAudio { .. }) {
        validate_filters(ffmpeg, &["loudnorm"])?;
    }
    if matches!(&recipe.operation, Operation::Concatenate { .. }) {
        validate_filters(ffmpeg, &["concat"])?;
    }
    if matches!(&recipe.operation, Operation::Thumbnail { .. }) {
        validate_filters(ffmpeg, &["scale"])?;
    }
    if matches!(&recipe.operation, Operation::ContactSheet { .. }) {
        validate_filters(ffmpeg, &["fps", "scale", "tile"])?;
    }
    if matches!(&recipe.operation, Operation::BurnSubtitles { .. }) {
        validate_filters(ffmpeg, &["subtitles"])?;
    }
    let recipe_digest = digest_json(&recipe)?;
    let constraints_digest = digest_json(&recipe.constraints)?;
    let operation = recipe.operation.clone();
    let inputs = PlanInputs {
        recipe,
        recipe_digest,
        constraints_digest,
        inputs: inspected_inputs,
        output,
        backend,
    };
    match operation {
        Operation::Clip { .. } => compile_clip(inputs),
        Operation::Transcode { .. } => compile_transcode(inputs),
        Operation::Resize { .. } => compile_resize(inputs),
        Operation::ExtractAudio { .. } => compile_extract_audio(inputs),
        Operation::NormalizeAudio { .. } => compile_normalize_audio(inputs),
        Operation::Concatenate { .. } => compile_concatenate(inputs),
        Operation::Thumbnail { .. } => compile_thumbnail(inputs),
        Operation::ContactSheet { .. } => compile_contact_sheet(inputs),
        Operation::BurnSubtitles { .. } => compile_burn_subtitles(inputs),
    }
}

pub fn write_new_plan(path: &Path, json: &str) -> Result<(), AvpactError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|source| AvpactError::PlanWrite {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(json.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|source| AvpactError::PlanWrite {
            path: path.to_path_buf(),
            source,
        })
}

pub fn read_plan(path: &Path) -> Result<Plan, AvpactError> {
    let bytes = read_bounded_document(path, MAX_PLAN_DOCUMENT_BYTES).map_err(|source| {
        AvpactError::PlanRead {
            path: path.to_path_buf(),
            source,
        }
    })?;
    if bytes.len() as u64 > MAX_PLAN_DOCUMENT_BYTES {
        return Err(AvpactError::PlanInvalid {
            message: format!(
                "plan exceeds the {} byte document limit",
                MAX_PLAN_DOCUMENT_BYTES
            ),
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| AvpactError::PlanInvalid {
        message: source.to_string(),
    })
}

pub fn plan_digest(plan: &Plan) -> Result<String, AvpactError> {
    digest_json(plan)
}

pub fn validate_plan(plan: &Plan) -> Result<Vec<String>, AvpactError> {
    if plan.schema_version != crate::PLAN_SCHEMA_VERSION {
        return invalid_plan(format!(
            "unsupported schema_version {:?}; expected {:?}",
            plan.schema_version,
            crate::PLAN_SCHEMA_VERSION
        ));
    }
    if plan.backend.name != "ffmpeg" {
        return invalid_plan("backend name must be ffmpeg");
    }
    if plan.backend.version.is_empty() || plan.backend.library_versions.is_empty() {
        return invalid_plan("backend build identity is incomplete");
    }
    if !is_sha256_digest(&plan.recipe_digest) || !is_sha256_digest(&plan.constraints_digest) {
        return invalid_plan("recipe and constraints digests must be lowercase SHA-256 values");
    }
    if plan.warnings.len() > MAX_PLAN_WARNINGS {
        return invalid_plan("plan contains too many warnings");
    }
    if plan.warnings.iter().any(|warning| {
        warning.code.len() > 64 || warning.message.chars().count() > MAX_WARNING_CHARACTERS
    }) {
        return invalid_plan("plan warning fields exceed their bounds");
    }
    if !plan.output.atomic_publish {
        return invalid_plan("atomic_publish must be true");
    }
    if plan.inputs.is_empty() {
        return invalid_plan("plan must contain at least one input");
    }
    if plan.inputs.iter().any(|input| {
        input.schema_version != crate::INSPECTION_SCHEMA_VERSION
            || input.backend.name != "ffprobe"
            || input.backend.version.is_empty()
            || !is_sha256_digest(&input.source.sha256)
    }) {
        return invalid_plan("input inspection identity is incomplete or unsupported");
    }
    if plan.inputs.iter().any(|input| {
        plan.output.path == input.source.path || plan.output.temporary_path == input.source.path
    }) || plan.output.temporary_path == plan.output.path
    {
        return invalid_plan("input, output, and temporary output paths must be distinct");
    }
    if plan.output.path.parent() != plan.output.temporary_path.parent() {
        return invalid_plan("temporary output must be adjacent to the final output");
    }
    let expected_resources = estimate_resources(
        &plan.inputs,
        &plan.expected,
        plan.resources.max_output_bytes,
        plan.resources.max_temporary_bytes,
        plan.resources.max_runtime_ms,
    );
    if plan.resources != expected_resources {
        return invalid_plan("resource estimates or limits are inconsistent");
    }
    if plan.resources.estimated_output_bytes > plan.resources.max_output_bytes
        || plan.resources.estimated_temporary_bytes > plan.resources.max_temporary_bytes
        || plan.resources.max_runtime_ms < 100
    {
        return invalid_plan("resource estimates exceed plan limits");
    }
    let expected_id = expected_plan_id(plan)?;
    if plan.id != expected_id {
        return invalid_plan(format!(
            "plan id mismatch; expected {expected_id}, found {}",
            plan.id
        ));
    }
    let expected_temporary = temporary_output_path(&plan.output.path, &plan.id)?;
    if plan.output.temporary_path != expected_temporary {
        return invalid_plan("temporary output path does not match the plan id");
    }

    let expected_argv = match &plan.operation {
        PlannedOperation::Clip {
            start_ms,
            end_ms,
            duration_ms,
            accuracy: ClipAccuracy::ExactTranscode,
        } => validate_clip_plan(plan, *start_ms, *end_ms, *duration_ms)?,
        PlannedOperation::Transcode => validate_transcode_plan(plan)?,
        PlannedOperation::Resize {
            requested_width,
            requested_height,
            output_width,
            output_height,
            mode,
            rotation,
        } => validate_resize_plan(
            plan,
            *requested_width,
            *requested_height,
            *output_width,
            *output_height,
            *mode,
            *rotation,
        )?,
        PlannedOperation::ExtractAudio => validate_extract_audio_plan(plan)?,
        PlannedOperation::NormalizeAudio {
            target_lufs_x100,
            tolerance_lu_x100,
        } => validate_normalize_audio_plan(plan, *target_lufs_x100, *tolerance_lu_x100)?,
        PlannedOperation::Concatenate {
            segment_count,
            duration_ms,
        } => validate_concatenate_plan(plan, *segment_count, *duration_ms)?,
        PlannedOperation::Thumbnail {
            at_ms,
            width,
            height,
        } => validate_thumbnail_plan(plan, *at_ms, *width, *height)?,
        PlannedOperation::ContactSheet {
            interval_ms,
            columns,
            rows,
            width,
            height,
        } => validate_contact_sheet_plan(plan, *interval_ms, *columns, *rows, *width, *height)?,
        PlannedOperation::BurnSubtitles => validate_burn_subtitles_plan(plan)?,
    };
    if plan.backend.argv != expected_argv {
        return invalid_plan("backend argv does not match the typed plan");
    }
    Ok(expected_argv)
}

fn validate_clip_plan(
    plan: &Plan,
    start_ms: u64,
    end_ms: u64,
    duration_ms: u64,
) -> Result<Vec<String>, AvpactError> {
    if start_ms >= end_ms || end_ms - start_ms != duration_ms {
        return invalid_plan("clip time range is inconsistent");
    }
    let input = single_input(plan)?;
    if input
        .format
        .duration_ms
        .is_none_or(|input_duration| end_ms > input_duration)
    {
        return invalid_plan("clip range exceeds the planned input duration");
    }
    if plan.expected.duration_ms != Some(duration_ms) {
        return invalid_plan("expected duration does not match the clip duration");
    }

    let selected_video = selected_kind(&plan.selected_streams, StreamKind::Video)?;
    let selected_audio = selected_optional_kind(&plan.selected_streams, StreamKind::Audio)?;
    if plan.selected_streams.len() != usize::from(selected_audio.is_some()) + 1 {
        return invalid_plan(
            "the clip plan may select only one video and one optional audio stream",
        );
    }
    if selected_video.source_index != 0
        || selected_audio.is_some_and(|selected| selected.source_index != 0)
    {
        return invalid_plan("clip streams must come from source index 0");
    }
    let video = input_stream(input, selected_video.stream_index, StreamKind::Video)?;
    let expected_selected = selected_streams(
        0,
        video,
        selected_audio
            .map(|selected| input_stream(input, selected.stream_index, StreamKind::Audio))
            .transpose()?,
    );
    if plan.selected_streams != expected_selected {
        return invalid_plan("selected stream policy or codec decision is inconsistent");
    }

    let dimensions = video
        .video
        .as_ref()
        .and_then(|video| video.width.zip(video.height))
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "selected video stream has unknown dimensions".to_owned(),
        })?;
    if plan.expected.video
        != Some(expected_video(
            "h264",
            dimensions.0,
            dimensions.1,
            stream_frame_rate(video),
        ))
    {
        return invalid_plan("expected video properties are inconsistent");
    }
    let expected_audio = selected_audio
        .map(|selected| -> Result<ExpectedAudio, AvpactError> {
            let input = input_stream(input, selected.stream_index, StreamKind::Audio)?;
            Ok(ExpectedAudio {
                codec: "aac".to_owned(),
                channels: input.audio.as_ref().and_then(|audio| audio.channels),
                loudness: None,
            })
        })
        .transpose()?;
    if plan.expected.audio != expected_audio {
        return invalid_plan("expected audio properties are inconsistent");
    }

    let expected_checks =
        verification_checks_with_size(&plan.expected, plan.resources.max_output_bytes);
    if plan.verification_checks != expected_checks {
        return invalid_plan("verification checks are inconsistent with expected output");
    }
    let expected_argv = clip_argv(
        &input.source.path,
        &plan.output.temporary_path,
        start_ms,
        duration_ms,
        selected_video.stream_index,
        selected_audio.map(|stream| stream.stream_index),
    );
    Ok(expected_argv)
}

fn validate_transcode_plan(plan: &Plan) -> Result<Vec<String>, AvpactError> {
    let input = single_input(plan)?;
    let video = default_video(input)?;
    let dimensions = stream_dimensions_for_plan(video)?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio);
    validate_web_expectations(plan, input, video, audio, dimensions)?;
    Ok(transcode_argv(
        &input.source.path,
        &plan.output.temporary_path,
        video.index,
        audio.map(|stream| stream.index),
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn validate_resize_plan(
    plan: &Plan,
    requested_width: u32,
    requested_height: u32,
    output_width: u32,
    output_height: u32,
    mode: ResizeMode,
    rotation: Rotation,
) -> Result<Vec<String>, AvpactError> {
    let input = single_input(plan)?;
    let video = default_video(input)?;
    let (source_width, source_height) = stream_dimensions_for_plan(video)?;
    let (rotated_width, rotated_height) = match rotation {
        Rotation::Clockwise90 | Rotation::CounterClockwise90 => (source_height, source_width),
        Rotation::None | Rotation::Clockwise180 => (source_width, source_height),
    };
    let expected_dimensions = match mode {
        ResizeMode::Fit => fit_dimensions(
            rotated_width,
            rotated_height,
            requested_width,
            requested_height,
        ),
        ResizeMode::Stretch | ResizeMode::Crop | ResizeMode::Pad => {
            (requested_width, requested_height)
        }
    };
    if expected_dimensions != (output_width, output_height) {
        return invalid_plan("resize output dimensions are inconsistent");
    }
    let audio = select_default_stream(&input.streams, StreamKind::Audio);
    validate_web_expectations(plan, input, video, audio, expected_dimensions)?;
    let filter = resize_filter(
        requested_width,
        requested_height,
        output_width,
        output_height,
        mode,
        rotation,
    );
    Ok(transcode_argv(
        &input.source.path,
        &plan.output.temporary_path,
        video.index,
        audio.map(|stream| stream.index),
        Some(&filter),
    ))
}

fn validate_extract_audio_plan(plan: &Plan) -> Result<Vec<String>, AvpactError> {
    let input = single_input(plan)?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio).ok_or_else(|| {
        AvpactError::PlanInvalid {
            message: "audio extraction input has no audio stream".to_owned(),
        }
    })?;
    let expected_selected = selected_audio_stream(0, audio);
    if plan.selected_streams != expected_selected {
        return invalid_plan("audio extraction stream selection is inconsistent");
    }
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: input.format.duration_ms,
        duration_tolerance_ms: plan.expected.duration_tolerance_ms,
        video: None,
        audio: Some(ExpectedAudio {
            codec: "aac".to_owned(),
            channels: audio.audio.as_ref().and_then(|audio| audio.channels),
            loudness: None,
        }),
    };
    validate_expected_output(plan, &expected)?;
    Ok(extract_audio_argv(
        &input.source.path,
        &plan.output.temporary_path,
        audio.index,
    ))
}

fn validate_normalize_audio_plan(
    plan: &Plan,
    target_lufs_x100: i32,
    tolerance_lu_x100: u32,
) -> Result<Vec<String>, AvpactError> {
    if !(-7000..=-500).contains(&target_lufs_x100)
        || tolerance_lu_x100 == 0
        || tolerance_lu_x100 > 500
    {
        return invalid_plan("normalization loudness target or tolerance is out of range");
    }
    let input = single_input(plan)?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio).ok_or_else(|| {
        AvpactError::PlanInvalid {
            message: "audio normalization input has no audio stream".to_owned(),
        }
    })?;
    if plan.selected_streams != selected_audio_stream(0, audio) {
        return invalid_plan("audio normalization stream selection is inconsistent");
    }
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: input.format.duration_ms,
        duration_tolerance_ms: plan.expected.duration_tolerance_ms,
        video: None,
        audio: Some(ExpectedAudio {
            codec: "aac".to_owned(),
            channels: audio.audio.as_ref().and_then(|audio| audio.channels),
            loudness: Some(ExpectedLoudness {
                integrated_lufs_x100: target_lufs_x100,
                tolerance_lu_x100,
            }),
        }),
    };
    validate_expected_output(plan, &expected)?;
    Ok(normalize_audio_argv(
        &input.source.path,
        &plan.output.temporary_path,
        audio.index,
        target_lufs_x100,
    ))
}

fn validate_concatenate_plan(
    plan: &Plan,
    segment_count: usize,
    planned_duration_ms: Option<u64>,
) -> Result<Vec<String>, AvpactError> {
    if !(2..=64).contains(&segment_count) || plan.inputs.len() != segment_count {
        return invalid_plan("concatenate segment count is inconsistent");
    }
    let mut selected = Vec::new();
    let mut input_paths = Vec::new();
    let mut video_indices = Vec::new();
    let mut audio_indices = Vec::new();
    let mut dimensions = None;
    let mut frame_rate_policy = None;
    let mut duration_ms = Some(0_u64);
    for (source_index, input) in plan.inputs.iter().enumerate() {
        let video = default_video(input)?;
        let current_dimensions = stream_dimensions_for_plan(video)?;
        if dimensions.is_some_and(|expected| expected != current_dimensions) {
            return invalid_plan("concatenate input dimensions differ");
        }
        dimensions.get_or_insert(current_dimensions);
        let current_frame_rate = stream_frame_rate(video);
        if frame_rate_policy
            .as_ref()
            .is_some_and(|expected| expected != &current_frame_rate)
        {
            return invalid_plan("concatenate input frame rates differ");
        }
        frame_rate_policy.get_or_insert(current_frame_rate);
        let audio = select_default_stream(&input.streams, StreamKind::Audio);
        selected.extend(selected_streams_for_concat(source_index, video, audio));
        input_paths.push(input.source.path.clone());
        video_indices.push(video.index);
        audio_indices.push(audio.map(|stream| stream.index));
        duration_ms = duration_ms
            .zip(input.format.duration_ms)
            .and_then(|(total, duration)| total.checked_add(duration));
    }
    if duration_ms != planned_duration_ms {
        return invalid_plan("concatenate duration is inconsistent");
    }
    if plan.selected_streams != selected {
        return invalid_plan("concatenate stream selection is inconsistent");
    }
    let has_audio = audio_indices.iter().all(Option::is_some);
    if !has_audio && audio_indices.iter().any(Option::is_some) {
        return invalid_plan("concatenate audio presence differs across inputs");
    }
    let (width, height) = dimensions.expect("at least two video inputs");
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms,
        duration_tolerance_ms: plan.expected.duration_tolerance_ms,
        video: Some(expected_video(
            "h264",
            width,
            height,
            frame_rate_policy.flatten(),
        )),
        audio: has_audio.then(|| ExpectedAudio {
            codec: "aac".to_owned(),
            channels: Some(2),
            loudness: None,
        }),
    };
    validate_expected_output(plan, &expected)?;
    Ok(concatenate_argv(
        &input_paths,
        &plan.output.temporary_path,
        &video_indices,
        &audio_indices,
        has_audio,
    ))
}

fn validate_thumbnail_plan(
    plan: &Plan,
    at_ms: u64,
    width: u32,
    height: u32,
) -> Result<Vec<String>, AvpactError> {
    let input = single_input(plan)?;
    if input
        .format
        .duration_ms
        .is_some_and(|duration| at_ms >= duration)
    {
        return invalid_plan("thumbnail timestamp exceeds input duration");
    }
    let video = default_video(input)?;
    let (source_width, source_height) = stream_dimensions_for_plan(video)?;
    let expected_height =
        even_floor(u64::from(source_height) * u64::from(width) / u64::from(source_width));
    if height != expected_height {
        return invalid_plan("thumbnail height is inconsistent");
    }
    validate_image_expectations(plan, video, width, height)?;
    Ok(thumbnail_argv(
        &input.source.path,
        &plan.output.temporary_path,
        video.index,
        at_ms,
        width,
        height,
    ))
}

#[allow(clippy::too_many_arguments)]
fn validate_contact_sheet_plan(
    plan: &Plan,
    interval_ms: u64,
    columns: u32,
    rows: u32,
    width: u32,
    height: u32,
) -> Result<Vec<String>, AvpactError> {
    if interval_ms < 100
        || columns == 0
        || rows == 0
        || columns.saturating_mul(rows) > 100
        || width % columns != 0
    {
        return invalid_plan("contact sheet parameters are inconsistent");
    }
    let input = single_input(plan)?;
    let video = default_video(input)?;
    let (source_width, source_height) = stream_dimensions_for_plan(video)?;
    let cell_width = width / columns;
    let cell_height =
        even_floor(u64::from(source_height) * u64::from(cell_width) / u64::from(source_width));
    if height != cell_height.saturating_mul(rows) {
        return invalid_plan("contact sheet height is inconsistent");
    }
    validate_image_expectations(plan, video, width, height)?;
    Ok(contact_sheet_argv(
        &input.source.path,
        &plan.output.temporary_path,
        video.index,
        interval_ms,
        columns,
        rows,
        cell_width,
        cell_height,
    ))
}

fn validate_image_expectations(
    plan: &Plan,
    video: &StreamSummary,
    width: u32,
    height: u32,
) -> Result<(), AvpactError> {
    if plan.selected_streams != selected_image_stream(0, video) {
        return invalid_plan("image stream selection is inconsistent");
    }
    validate_expected_output(plan, &expected_jpeg(width, height))
}

fn validate_burn_subtitles_plan(plan: &Plan) -> Result<Vec<String>, AvpactError> {
    if plan.inputs.len() != 2 {
        return invalid_plan("subtitle burn-in requires exactly two inputs");
    }
    let media = &plan.inputs[0];
    let subtitle_input = &plan.inputs[1];
    let video = default_video(media)?;
    let subtitle = select_default_stream(&subtitle_input.streams, StreamKind::Subtitle)
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "subtitle input has no subtitle stream".to_owned(),
        })?;
    let audio = select_default_stream(&media.streams, StreamKind::Audio);
    let mut expected_selected = selected_streams(0, video, audio);
    expected_selected.push(selected_subtitle_stream(1, subtitle));
    if plan.selected_streams != expected_selected {
        return invalid_plan("subtitle burn-in stream selection is inconsistent");
    }
    let dimensions = stream_dimensions_for_plan(video)?;
    let expected = expected_web_output(
        media,
        video,
        dimensions.0,
        dimensions.1,
        audio,
        plan.expected.duration_tolerance_ms,
    );
    validate_expected_output(plan, &expected)?;
    let filter = subtitles_filter(&subtitle_input.source.path);
    Ok(transcode_argv(
        &media.source.path,
        &plan.output.temporary_path,
        video.index,
        audio.map(|stream| stream.index),
        Some(&filter),
    ))
}

fn validate_web_expectations(
    plan: &Plan,
    input: &InspectionReport,
    video: &StreamSummary,
    audio: Option<&StreamSummary>,
    dimensions: (u32, u32),
) -> Result<(), AvpactError> {
    let expected_selected = selected_streams(0, video, audio);
    if plan.selected_streams != expected_selected {
        return invalid_plan("web stream selection is inconsistent");
    }
    let expected = expected_web_output(
        input,
        video,
        dimensions.0,
        dimensions.1,
        audio,
        plan.expected.duration_tolerance_ms,
    );
    validate_expected_output(plan, &expected)
}

fn validate_expected_output(plan: &Plan, expected: &ExpectedOutput) -> Result<(), AvpactError> {
    if plan.expected.duration_tolerance_ms > 10_000 {
        return invalid_plan("duration tolerance exceeds 10000ms");
    }
    if &plan.expected != expected {
        return invalid_plan("expected output properties are inconsistent");
    }
    if plan.verification_checks
        != verification_checks_with_size(expected, plan.resources.max_output_bytes)
    {
        return invalid_plan("verification checks are inconsistent with expected output");
    }
    Ok(())
}

fn default_video(input: &InspectionReport) -> Result<&StreamSummary, AvpactError> {
    select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::PlanInvalid {
            message: "operation input has no video stream".to_owned(),
        }
    })
}

fn stream_dimensions_for_plan(stream: &StreamSummary) -> Result<(u32, u32), AvpactError> {
    stream
        .video
        .as_ref()
        .and_then(|video| video.width.zip(video.height))
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: format!("video stream {} has unknown dimensions", stream.index),
        })
}

fn validate_recipe(recipe: &Recipe) -> Result<(), AvpactError> {
    if recipe.schema_version != crate::RECIPE_SCHEMA_VERSION {
        return Err(AvpactError::RecipeInvalid {
            message: format!(
                "unsupported schema_version {:?}; expected {:?}",
                recipe.schema_version,
                crate::RECIPE_SCHEMA_VERSION
            ),
        });
    }

    match &recipe.operation {
        Operation::Clip {
            start_ms, end_ms, ..
        } if start_ms >= end_ms => Err(AvpactError::RecipeInvalid {
            message: "clip start_ms must be less than end_ms".to_owned(),
        }),
        Operation::Resize { width, height, .. }
            if *width < 2
                || *height < 2
                || *width > 16_384
                || *height > 16_384
                || *width % 2 != 0
                || *height % 2 != 0 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "resize width and height must be even values between 2 and 16384"
                    .to_owned(),
            })
        }
        Operation::NormalizeAudio {
            target_lufs,
            tolerance_lu_x100,
            ..
        } if *target_lufs < -70
            || *target_lufs > -5
            || *tolerance_lu_x100 == 0
            || *tolerance_lu_x100 > 500 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "normalize_audio target_lufs must be -70..=-5 and tolerance_lu_x100 must be 1..=500".to_owned(),
            })
        }
        Operation::Concatenate { inputs, .. }
            if inputs.len() < 2 || inputs.len() > 64 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "concatenate requires between 2 and 64 inputs".to_owned(),
            })
        }
        Operation::Thumbnail { width, .. }
            if *width < 2 || *width > 8_192 || *width % 2 != 0 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "thumbnail width must be an even value between 2 and 8192".to_owned(),
            })
        }
        Operation::ContactSheet {
            interval_ms,
            columns,
            rows,
            width,
            ..
        } if *interval_ms < 100
            || *interval_ms > 3_600_000
            || *columns == 0
            || *rows == 0
            || columns.saturating_mul(*rows) > 100
            || *width < 2
            || *width > 16_384
            || *width % *columns != 0
            || (*width / *columns) % 2 != 0 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "contact_sheet requires interval_ms 100..=3600000, 1..=100 cells, and an even cell width".to_owned(),
            })
        }
        _ if recipe.constraints.duration_tolerance_ms > 10_000 => Err(AvpactError::RecipeInvalid {
            message: "duration_tolerance_ms must not exceed 10000".to_owned(),
        }),
        _ if recipe.constraints.max_output_bytes == 0
            || recipe.constraints.max_temporary_bytes
                < recipe.constraints.max_output_bytes
            || recipe.constraints.max_runtime_ms < 100 =>
        {
            Err(AvpactError::RecipeInvalid {
                message: "resource budgets require max_output_bytes > 0, max_temporary_bytes >= max_output_bytes, and max_runtime_ms >= 100".to_owned(),
            })
        }
        _ => Ok(()),
    }
}

fn resolve_input(base: &Path, declared: &Path) -> PathBuf {
    if declared.is_absolute() {
        declared.to_path_buf()
    } else {
        base.join(declared)
    }
}

fn resolve_output(base: &Path, declared: &Path) -> Result<PathBuf, AvpactError> {
    let unresolved = if declared.is_absolute() {
        declared.to_path_buf()
    } else {
        base.join(declared)
    };

    if unresolved.exists() {
        return unresolved
            .canonicalize()
            .map_err(|source| AvpactError::OutputPathInvalid {
                path: unresolved,
                message: source.to_string(),
            });
    }

    let file_name = unresolved
        .file_name()
        .ok_or_else(|| AvpactError::OutputPathInvalid {
            path: unresolved.clone(),
            message: "output must have a file name".to_owned(),
        })?;
    let parent = unresolved
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|source| AvpactError::OutputPathInvalid {
            path: unresolved.clone(),
            message: format!("output directory is unavailable: {source}"),
        })?;
    Ok(parent.join(file_name))
}

fn validate_paths(
    input: &SourceIdentity,
    output: &Path,
    overwrite: OverwritePolicy,
) -> Result<(), AvpactError> {
    if input.path == output {
        return Err(AvpactError::InputOutputConflict {
            path: output.to_path_buf(),
        });
    }
    if output.exists() && matches!(overwrite, OverwritePolicy::Deny) {
        return Err(AvpactError::OutputExists {
            path: output.to_path_buf(),
        });
    }
    Ok(())
}

pub(crate) fn backend_identity(ffmpeg: &Path) -> Result<FfmpegBuildIdentity, AvpactError> {
    let output = Command::new(ffmpeg)
        .arg("-version")
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: ffmpeg.display().to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffmpeg.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let version = lines
        .next()
        .unwrap_or("ffmpeg version unknown")
        .trim()
        .to_owned();
    let mut configuration = String::new();
    let mut library_versions = BTreeMap::new();
    for line in lines {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("configuration:") {
            configuration = value.trim().to_owned();
            continue;
        }
        if let Some((name, value)) = trimmed.split_once(char::is_whitespace) {
            if name.starts_with("lib") {
                library_versions.insert(
                    name.to_owned(),
                    value.split_whitespace().collect::<Vec<_>>().join(" "),
                );
            }
        }
    }
    Ok(FfmpegBuildIdentity {
        version,
        configuration,
        library_versions,
    })
}

fn validate_encoders(ffmpeg: &Path, required: &[&str]) -> Result<(), AvpactError> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: ffmpeg.display().to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffmpeg.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    for encoder in required {
        if !has_encoder(&listing, encoder) {
            return Err(AvpactError::Unsupported {
                message: format!("operation requires the {encoder} FFmpeg encoder"),
            });
        }
    }
    Ok(())
}

fn has_encoder(listing: &str, name: &str) -> bool {
    listing.lines().any(|line| {
        line.split_whitespace()
            .nth(1)
            .is_some_and(|candidate| candidate == name)
    })
}

fn validate_filters(ffmpeg: &Path, required: &[&str]) -> Result<(), AvpactError> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-filters"])
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: ffmpeg.display().to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffmpeg.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    for filter in required {
        if !listing.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .is_some_and(|candidate| candidate == *filter)
        }) {
            return Err(AvpactError::Unsupported {
                message: format!("operation requires the {filter} FFmpeg filter"),
            });
        }
    }
    Ok(())
}

fn compile_clip(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let (start_ms, end_ms) = match &inputs.recipe.operation {
        Operation::Clip {
            start_ms, end_ms, ..
        } => (*start_ms, *end_ms),
        _ => {
            return Err(AvpactError::PlanInvalid {
                message: "clip compiler received another operation".to_owned(),
            });
        }
    };
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "clip planning requires one input".to_owned(),
        })?;
    let source_duration = input
        .format
        .duration_ms
        .ok_or_else(|| AvpactError::Unsupported {
            message: "clip planning requires a known input duration".to_owned(),
        })?;
    if end_ms > source_duration {
        return Err(AvpactError::RecipeInvalid {
            message: format!("clip end_ms {end_ms} exceeds input duration {source_duration}"),
        });
    }

    let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "the web clip target requires a video stream".to_owned(),
        }
    })?;
    let video_summary = video
        .video
        .as_ref()
        .ok_or_else(|| AvpactError::Unsupported {
            message: format!("video stream {} has no dimension metadata", video.index),
        })?;
    let (width, height) = match (video_summary.width, video_summary.height) {
        (Some(width), Some(height)) => (width, height),
        _ => {
            return Err(AvpactError::Unsupported {
                message: format!("video stream {} has unknown dimensions", video.index),
            });
        }
    };
    let audio = select_default_stream(&input.streams, StreamKind::Audio);
    let duration_ms = end_ms - start_ms;
    let selected_streams = selected_streams(0, video, audio);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected_audio = audio.map(|stream| ExpectedAudio {
        codec: "aac".to_owned(),
        channels: stream.audio.as_ref().and_then(|audio| audio.channels),
        loudness: None,
    });
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: Some(duration_ms),
        duration_tolerance_ms: inputs.recipe.constraints.duration_tolerance_ms,
        video: Some(expected_video(
            "h264",
            width,
            height,
            stream_frame_rate(video),
        )),
        audio: expected_audio.clone(),
    };
    let verification_checks = verification_checks(&expected);
    let operation = PlannedOperation::Clip {
        start_ms,
        end_ms,
        duration_ms,
        accuracy: ClipAccuracy::ExactTranscode,
    };
    let input_path = input.source.path.clone();
    let video_index = video.index;
    let audio_index = audio.map(|stream| stream.index);
    assemble_plan(
        inputs,
        operation,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            clip_argv(
                &input_path,
                temporary_path,
                start_ms,
                duration_ms,
                video_index,
                audio_index,
            )
        },
    )
}

fn compile_transcode(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "transcode planning requires one input".to_owned(),
        })?;
    let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "the web transcode target requires a video stream".to_owned(),
        }
    })?;
    let (width, height) = stream_dimensions(video)?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio);
    let selected_streams = selected_streams(0, video, audio);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = expected_web_output(
        input,
        video,
        width,
        height,
        audio,
        inputs.recipe.constraints.duration_tolerance_ms,
    );
    let verification_checks = verification_checks(&expected);
    let input_path = input.source.path.clone();
    let video_index = video.index;
    let audio_index = audio.map(|stream| stream.index);

    assemble_plan(
        inputs,
        PlannedOperation::Transcode,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            transcode_argv(&input_path, temporary_path, video_index, audio_index, None)
        },
    )
}

fn compile_resize(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let (requested_width, requested_height, mode, rotation) = match &inputs.recipe.operation {
        Operation::Resize {
            width,
            height,
            mode,
            rotation,
            ..
        } => (*width, *height, *mode, *rotation),
        _ => {
            return Err(AvpactError::PlanInvalid {
                message: "resize compiler received another operation".to_owned(),
            });
        }
    };
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "resize planning requires one input".to_owned(),
        })?;
    let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "resize requires a video stream".to_owned(),
        }
    })?;
    let (source_width, source_height) = stream_dimensions(video)?;
    let (rotated_width, rotated_height) = match rotation {
        Rotation::Clockwise90 | Rotation::CounterClockwise90 => (source_height, source_width),
        Rotation::None | Rotation::Clockwise180 => (source_width, source_height),
    };
    let (output_width, output_height) = match mode {
        ResizeMode::Fit => fit_dimensions(
            rotated_width,
            rotated_height,
            requested_width,
            requested_height,
        ),
        ResizeMode::Stretch | ResizeMode::Crop | ResizeMode::Pad => {
            (requested_width, requested_height)
        }
    };
    let filter = resize_filter(
        requested_width,
        requested_height,
        output_width,
        output_height,
        mode,
        rotation,
    );
    let audio = select_default_stream(&input.streams, StreamKind::Audio);
    let selected_streams = selected_streams(0, video, audio);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = expected_web_output(
        input,
        video,
        output_width,
        output_height,
        audio,
        inputs.recipe.constraints.duration_tolerance_ms,
    );
    let verification_checks = verification_checks(&expected);
    let operation = PlannedOperation::Resize {
        requested_width,
        requested_height,
        output_width,
        output_height,
        mode,
        rotation,
    };
    let input_path = input.source.path.clone();
    let video_index = video.index;
    let audio_index = audio.map(|stream| stream.index);

    assemble_plan(
        inputs,
        operation,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            transcode_argv(
                &input_path,
                temporary_path,
                video_index,
                audio_index,
                Some(&filter),
            )
        },
    )
}

fn compile_extract_audio(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "audio extraction planning requires one input".to_owned(),
        })?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "audio extraction requires an audio stream".to_owned(),
        }
    })?;
    let selected_streams = selected_audio_stream(0, audio);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: input.format.duration_ms,
        duration_tolerance_ms: inputs.recipe.constraints.duration_tolerance_ms,
        video: None,
        audio: Some(ExpectedAudio {
            codec: "aac".to_owned(),
            channels: audio.audio.as_ref().and_then(|audio| audio.channels),
            loudness: None,
        }),
    };
    let verification_checks = verification_checks(&expected);
    let input_path = input.source.path.clone();
    let audio_index = audio.index;

    assemble_plan(
        inputs,
        PlannedOperation::ExtractAudio,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| extract_audio_argv(&input_path, temporary_path, audio_index),
    )
}

fn compile_normalize_audio(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let (target_lufs_x100, tolerance_lu_x100) = match &inputs.recipe.operation {
        Operation::NormalizeAudio {
            target_lufs,
            tolerance_lu_x100,
            ..
        } => (target_lufs.saturating_mul(100), *tolerance_lu_x100),
        _ => {
            return Err(AvpactError::PlanInvalid {
                message: "normalize compiler received another operation".to_owned(),
            });
        }
    };
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "normalization planning requires one input".to_owned(),
        })?;
    let audio = select_default_stream(&input.streams, StreamKind::Audio).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "audio normalization requires an audio stream".to_owned(),
        }
    })?;
    let selected_streams = selected_audio_stream(0, audio);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: input.format.duration_ms,
        duration_tolerance_ms: inputs.recipe.constraints.duration_tolerance_ms,
        video: None,
        audio: Some(ExpectedAudio {
            codec: "aac".to_owned(),
            channels: audio.audio.as_ref().and_then(|audio| audio.channels),
            loudness: Some(ExpectedLoudness {
                integrated_lufs_x100: target_lufs_x100,
                tolerance_lu_x100,
            }),
        }),
    };
    let verification_checks = verification_checks(&expected);
    let input_path = input.source.path.clone();
    let audio_index = audio.index;

    assemble_plan(
        inputs,
        PlannedOperation::NormalizeAudio {
            target_lufs_x100,
            tolerance_lu_x100,
        },
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            normalize_audio_argv(&input_path, temporary_path, audio_index, target_lufs_x100)
        },
    )
}

fn compile_concatenate(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    if inputs.inputs.len() < 2 {
        return Err(AvpactError::RecipeInvalid {
            message: "concatenate requires at least two inputs".to_owned(),
        });
    }
    let mut selected_streams = Vec::new();
    let mut warnings = Vec::new();
    let mut input_paths = Vec::new();
    let mut video_indices = Vec::new();
    let mut audio_indices = Vec::new();
    let mut dimensions = None;
    let mut frame_rate_policy = None;
    let mut duration_ms = Some(0_u64);

    for (source_index, input) in inputs.inputs.iter().enumerate() {
        let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
            AvpactError::Unsupported {
                message: format!("concatenate input {source_index} has no video stream"),
            }
        })?;
        let current_dimensions = stream_dimensions(video)?;
        if dimensions.is_some_and(|expected| expected != current_dimensions) {
            return Err(AvpactError::Unsupported {
                message: format!(
                    "concatenate input {source_index} dimensions {current_dimensions:?} differ from the first input"
                ),
            });
        }
        dimensions.get_or_insert(current_dimensions);
        let current_frame_rate = stream_frame_rate(video);
        if frame_rate_policy
            .as_ref()
            .is_some_and(|expected| expected != &current_frame_rate)
        {
            return Err(AvpactError::Unsupported {
                message: format!(
                    "concatenate input {source_index} frame rate {current_frame_rate:?} differs from the first input"
                ),
            });
        }
        frame_rate_policy.get_or_insert(current_frame_rate);
        let audio = select_default_stream(&input.streams, StreamKind::Audio);
        input_paths.push(input.source.path.clone());
        video_indices.push(video.index);
        audio_indices.push(audio.map(|stream| stream.index));
        selected_streams.extend(selected_streams_for_concat(source_index, video, audio));
        duration_ms = duration_ms
            .zip(input.format.duration_ms)
            .and_then(|(total, duration)| total.checked_add(duration));
    }
    let has_audio = audio_indices.iter().all(Option::is_some);
    if !has_audio && audio_indices.iter().any(Option::is_some) {
        return Err(AvpactError::Unsupported {
            message: "concatenate requires either audio on every input or audio on none".to_owned(),
        });
    }
    for (source_index, input) in inputs.inputs.iter().enumerate() {
        warnings.extend(dropped_stream_warnings(
            source_index,
            &input.streams,
            &selected_streams,
        ));
    }
    let (width, height) = dimensions.expect("at least two inputs with video");
    let expected = ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms,
        duration_tolerance_ms: inputs.recipe.constraints.duration_tolerance_ms,
        video: Some(expected_video(
            "h264",
            width,
            height,
            frame_rate_policy.flatten(),
        )),
        audio: has_audio.then(|| ExpectedAudio {
            codec: "aac".to_owned(),
            channels: Some(2),
            loudness: None,
        }),
    };
    let verification_checks = verification_checks(&expected);
    let operation = PlannedOperation::Concatenate {
        segment_count: input_paths.len(),
        duration_ms,
    };

    assemble_plan(
        inputs,
        operation,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            concatenate_argv(
                &input_paths,
                temporary_path,
                &video_indices,
                &audio_indices,
                has_audio,
            )
        },
    )
}

fn compile_thumbnail(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let (at_ms, width) = match &inputs.recipe.operation {
        Operation::Thumbnail { at_ms, width, .. } => (*at_ms, *width),
        _ => {
            return Err(AvpactError::PlanInvalid {
                message: "thumbnail compiler received another operation".to_owned(),
            });
        }
    };
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "thumbnail planning requires one input".to_owned(),
        })?;
    if input
        .format
        .duration_ms
        .is_some_and(|duration| at_ms >= duration)
    {
        return Err(AvpactError::RecipeInvalid {
            message: format!("thumbnail at_ms {at_ms} must be less than input duration"),
        });
    }
    let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "thumbnail requires a video stream".to_owned(),
        }
    })?;
    let (source_width, source_height) = stream_dimensions(video)?;
    let height = even_floor(u64::from(source_height) * u64::from(width) / u64::from(source_width));
    let selected_streams = selected_image_stream(0, video);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = expected_jpeg(width, height);
    let verification_checks = verification_checks(&expected);
    let input_path = input.source.path.clone();
    let video_index = video.index;

    assemble_plan(
        inputs,
        PlannedOperation::Thumbnail {
            at_ms,
            width,
            height,
        },
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            thumbnail_argv(
                &input_path,
                temporary_path,
                video_index,
                at_ms,
                width,
                height,
            )
        },
    )
}

fn compile_contact_sheet(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    let (interval_ms, columns, rows, width) = match &inputs.recipe.operation {
        Operation::ContactSheet {
            interval_ms,
            columns,
            rows,
            width,
            ..
        } => (*interval_ms, *columns, *rows, *width),
        _ => {
            return Err(AvpactError::PlanInvalid {
                message: "contact sheet compiler received another operation".to_owned(),
            });
        }
    };
    let input = inputs
        .inputs
        .first()
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: "contact sheet planning requires one input".to_owned(),
        })?;
    let video = select_default_stream(&input.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "contact sheet requires a video stream".to_owned(),
        }
    })?;
    let (source_width, source_height) = stream_dimensions(video)?;
    let cell_width = width / columns;
    let cell_height =
        even_floor(u64::from(source_height) * u64::from(cell_width) / u64::from(source_width));
    let height = cell_height.saturating_mul(rows);
    let selected_streams = selected_image_stream(0, video);
    let warnings = dropped_stream_warnings(0, &input.streams, &selected_streams);
    let expected = expected_jpeg(width, height);
    let verification_checks = verification_checks(&expected);
    let input_path = input.source.path.clone();
    let video_index = video.index;

    assemble_plan(
        inputs,
        PlannedOperation::ContactSheet {
            interval_ms,
            columns,
            rows,
            width,
            height,
        },
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            contact_sheet_argv(
                &input_path,
                temporary_path,
                video_index,
                interval_ms,
                columns,
                rows,
                cell_width,
                cell_height,
            )
        },
    )
}

fn compile_burn_subtitles(inputs: PlanInputs) -> Result<Plan, AvpactError> {
    if inputs.inputs.len() != 2 {
        return Err(AvpactError::PlanInvalid {
            message: "subtitle burn-in requires media and subtitle inputs".to_owned(),
        });
    }
    let media = &inputs.inputs[0];
    let subtitle_input = &inputs.inputs[1];
    let video = select_default_stream(&media.streams, StreamKind::Video).ok_or_else(|| {
        AvpactError::Unsupported {
            message: "subtitle burn-in requires a video stream".to_owned(),
        }
    })?;
    let subtitle = select_default_stream(&subtitle_input.streams, StreamKind::Subtitle)
        .ok_or_else(|| AvpactError::Unsupported {
            message: "subtitle input has no subtitle stream".to_owned(),
        })?;
    let (width, height) = stream_dimensions(video)?;
    let audio = select_default_stream(&media.streams, StreamKind::Audio);
    let mut selected_streams = selected_streams(0, video, audio);
    selected_streams.push(selected_subtitle_stream(1, subtitle));
    let mut warnings = dropped_stream_warnings(0, &media.streams, &selected_streams);
    warnings.extend(dropped_stream_warnings(
        1,
        &subtitle_input.streams,
        &selected_streams,
    ));
    warnings.push(PlanWarning {
        code: "subtitles_burned".to_owned(),
        message:
            "subtitle text is rendered into video pixels and is not retained as a subtitle stream"
                .to_owned(),
    });
    let expected = expected_web_output(
        media,
        video,
        width,
        height,
        audio,
        inputs.recipe.constraints.duration_tolerance_ms,
    );
    let verification_checks = verification_checks(&expected);
    let media_path = media.source.path.clone();
    let subtitle_path = subtitle_input.source.path.clone();
    let video_index = video.index;
    let audio_index = audio.map(|stream| stream.index);
    let filter = subtitles_filter(&subtitle_path);

    assemble_plan(
        inputs,
        PlannedOperation::BurnSubtitles,
        selected_streams,
        expected,
        verification_checks,
        warnings,
        move |temporary_path| {
            transcode_argv(
                &media_path,
                temporary_path,
                video_index,
                audio_index,
                Some(&filter),
            )
        },
    )
}

fn assemble_plan<F>(
    inputs: PlanInputs,
    operation: PlannedOperation,
    selected_streams: Vec<SelectedStream>,
    expected: ExpectedOutput,
    verification_checks: Vec<VerificationCheck>,
    mut warnings: Vec<PlanWarning>,
    build_argv: F,
) -> Result<Plan, AvpactError>
where
    F: FnOnce(&Path) -> Vec<String>,
{
    warnings.insert(
        0,
        PlanWarning {
            code: "lossy_encoding".to_owned(),
            message: "selected streams are re-encoded with lossy target codecs; byte and perceptual identity are not preserved".to_owned(),
        },
    );
    for warning in &mut warnings {
        warning.message = warning
            .message
            .chars()
            .take(MAX_WARNING_CHARACTERS)
            .collect();
    }
    warnings.truncate(MAX_PLAN_WARNINGS);
    let resources = estimate_resources(
        &inputs.inputs,
        &expected,
        inputs.recipe.constraints.max_output_bytes,
        inputs.recipe.constraints.max_temporary_bytes,
        inputs.recipe.constraints.max_runtime_ms,
    );
    if resources.estimated_output_bytes > resources.max_output_bytes {
        return Err(AvpactError::RecipeInvalid {
            message: format!(
                "estimated output {} bytes exceeds max_output_bytes {}",
                resources.estimated_output_bytes, resources.max_output_bytes
            ),
        });
    }
    if resources.estimated_temporary_bytes > resources.max_temporary_bytes {
        return Err(AvpactError::RecipeInvalid {
            message: format!(
                "estimated temporary use {} bytes exceeds max_temporary_bytes {}",
                resources.estimated_temporary_bytes, resources.max_temporary_bytes
            ),
        });
    }
    let mut verification_checks = verification_checks;
    verification_checks.push(VerificationCheck::OutputSize {
        max_bytes: resources.max_output_bytes,
    });
    verification_checks.push(VerificationCheck::DistinctFromInputs);
    let identity_material = PlanIdentityMaterial {
        recipe_digest: &inputs.recipe_digest,
        constraints_digest: &inputs.constraints_digest,
        inputs: &inputs.inputs,
        output: &inputs.output,
        overwrite: inputs.recipe.constraints.overwrite,
        operation: &operation,
        selected_streams: &selected_streams,
        backend_version: &inputs.backend.version,
        backend_configuration: &inputs.backend.configuration,
        backend_library_versions: &inputs.backend.library_versions,
        expected: &expected,
        verification_checks: &verification_checks,
        warnings: &warnings,
        resources: &resources,
    };
    let id = format!("plan_{}", &digest_json(&identity_material)?[..32]);
    let temporary_path = temporary_output_path(&inputs.output, &id)?;
    let argv = build_argv(&temporary_path);

    Ok(Plan {
        schema_version: crate::PLAN_SCHEMA_VERSION.to_owned(),
        id,
        recipe_digest: inputs.recipe_digest,
        constraints_digest: inputs.constraints_digest,
        inputs: inputs.inputs,
        output: PlannedOutput {
            path: inputs.output,
            temporary_path,
            overwrite: inputs.recipe.constraints.overwrite,
            atomic_publish: true,
        },
        operation,
        selected_streams,
        backend: PlannedBackend {
            name: "ffmpeg".to_owned(),
            version: inputs.backend.version,
            configuration: inputs.backend.configuration,
            library_versions: inputs.backend.library_versions,
            argv,
        },
        expected,
        verification_checks,
        warnings,
        resources,
    })
}

fn select_default_stream(streams: &[StreamSummary], kind: StreamKind) -> Option<&StreamSummary> {
    streams
        .iter()
        .filter(|stream| stream.kind == kind)
        .find(|stream| stream.disposition.get("default") == Some(&1))
        .or_else(|| streams.iter().find(|stream| stream.kind == kind))
}

fn selected_streams(
    source_index: usize,
    video: &StreamSummary,
    audio: Option<&StreamSummary>,
) -> Vec<SelectedStream> {
    let mut selected = vec![SelectedStream {
        source_index,
        stream_index: video.index,
        kind: StreamKind::Video,
        reason: if video.disposition.get("default") == Some(&1) {
            "default video stream".to_owned()
        } else {
            "first video stream; no default video disposition".to_owned()
        },
        output_codec: "h264/libx264".to_owned(),
    }];
    if let Some(audio) = audio {
        selected.push(SelectedStream {
            source_index,
            stream_index: audio.index,
            kind: StreamKind::Audio,
            reason: if audio.disposition.get("default") == Some(&1) {
                "default audio stream".to_owned()
            } else {
                "first audio stream; no default audio disposition".to_owned()
            },
            output_codec: "aac".to_owned(),
        });
    }
    selected
}

fn selected_audio_stream(source_index: usize, audio: &StreamSummary) -> Vec<SelectedStream> {
    vec![SelectedStream {
        source_index,
        stream_index: audio.index,
        kind: StreamKind::Audio,
        reason: if audio.disposition.get("default") == Some(&1) {
            "default audio stream".to_owned()
        } else {
            "first audio stream; no default audio disposition".to_owned()
        },
        output_codec: "aac".to_owned(),
    }]
}

fn selected_streams_for_concat(
    source_index: usize,
    video: &StreamSummary,
    audio: Option<&StreamSummary>,
) -> Vec<SelectedStream> {
    selected_streams(source_index, video, audio)
}

fn selected_image_stream(source_index: usize, video: &StreamSummary) -> Vec<SelectedStream> {
    vec![SelectedStream {
        source_index,
        stream_index: video.index,
        kind: StreamKind::Video,
        reason: if video.disposition.get("default") == Some(&1) {
            "default video stream".to_owned()
        } else {
            "first video stream; no default video disposition".to_owned()
        },
        output_codec: "mjpeg".to_owned(),
    }]
}

fn selected_subtitle_stream(source_index: usize, subtitle: &StreamSummary) -> SelectedStream {
    SelectedStream {
        source_index,
        stream_index: subtitle.index,
        kind: StreamKind::Subtitle,
        reason: if subtitle.disposition.get("default") == Some(&1) {
            "default subtitle stream".to_owned()
        } else {
            "first subtitle stream; no default subtitle disposition".to_owned()
        },
        output_codec: "burned_into_video".to_owned(),
    }
}

fn stream_dimensions(stream: &StreamSummary) -> Result<(u32, u32), AvpactError> {
    stream
        .video
        .as_ref()
        .and_then(|video| video.width.zip(video.height))
        .ok_or_else(|| AvpactError::Unsupported {
            message: format!("video stream {} has unknown dimensions", stream.index),
        })
}

fn stream_frame_rate(stream: &StreamSummary) -> Option<String> {
    stream
        .video
        .as_ref()
        .and_then(|video| video.average_frame_rate.clone())
}

fn expected_video(
    codec: &str,
    width: u32,
    height: u32,
    average_frame_rate: Option<String>,
) -> ExpectedVideo {
    ExpectedVideo {
        codec: codec.to_owned(),
        width,
        height,
        sample_aspect_ratio: "1:1".to_owned(),
        average_frame_rate,
    }
}

fn expected_web_output(
    input: &InspectionReport,
    video: &StreamSummary,
    width: u32,
    height: u32,
    audio: Option<&StreamSummary>,
    duration_tolerance_ms: u64,
) -> ExpectedOutput {
    ExpectedOutput {
        container: "mov,mp4,m4a,3gp,3g2,mj2".to_owned(),
        duration_ms: input.format.duration_ms,
        duration_tolerance_ms,
        video: Some(expected_video(
            "h264",
            width,
            height,
            stream_frame_rate(video),
        )),
        audio: audio.map(|stream| ExpectedAudio {
            codec: "aac".to_owned(),
            channels: stream.audio.as_ref().and_then(|audio| audio.channels),
            loudness: None,
        }),
    }
}

fn expected_jpeg(width: u32, height: u32) -> ExpectedOutput {
    ExpectedOutput {
        container: "image2".to_owned(),
        duration_ms: None,
        duration_tolerance_ms: 0,
        video: Some(expected_video("mjpeg", width, height, None)),
        audio: None,
    }
}

fn estimate_resources(
    inputs: &[InspectionReport],
    expected: &ExpectedOutput,
    max_output_bytes: u64,
    max_temporary_bytes: u64,
    max_runtime_ms: u64,
) -> ResourcePlan {
    let input_bytes = inputs.iter().fold(0_u64, |total, input| {
        total.saturating_add(input.source.size_bytes)
    });
    let estimated_output_bytes = if let Some(duration_ms) = expected.duration_ms {
        let video_bitrate = if expected.video.is_some() {
            2_500_000_u64
        } else {
            0
        };
        let audio_bitrate = if expected.audio.is_some() {
            192_000_u64
        } else {
            0
        };
        duration_ms
            .saturating_mul(video_bitrate.saturating_add(audio_bitrate))
            .saturating_div(8_000)
            .max(64 * 1024)
    } else if let Some(video) = &expected.video {
        u64::from(video.width)
            .saturating_mul(u64::from(video.height))
            .max(64 * 1024)
    } else {
        input_bytes.max(64 * 1024)
    };
    let estimated_temporary_bytes = estimated_output_bytes.saturating_mul(6).saturating_div(5);
    let total_duration_ms = inputs.iter().fold(0_u64, |total, input| {
        total.saturating_add(input.format.duration_ms.unwrap_or(0))
    });
    let max_pixels = inputs
        .iter()
        .flat_map(|input| input.streams.iter())
        .filter_map(|stream| {
            stream
                .video
                .as_ref()
                .and_then(|video| video.width.zip(video.height))
                .map(|(width, height)| u64::from(width) * u64::from(height))
        })
        .max()
        .unwrap_or(0);
    let class = if total_duration_ms <= 60_000 && max_pixels <= 1920 * 1080 && inputs.len() <= 2 {
        ResourceClass::Light
    } else if total_duration_ms <= 30 * 60 * 1000 && max_pixels <= 3840 * 2160 && inputs.len() <= 16
    {
        ResourceClass::Standard
    } else {
        ResourceClass::Heavy
    };
    ResourcePlan {
        class,
        estimated_output_bytes,
        estimated_temporary_bytes,
        max_output_bytes,
        max_temporary_bytes,
        max_runtime_ms,
    }
}

fn fit_dimensions(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32) {
    let source_wider = u64::from(source_width) * u64::from(target_height)
        > u64::from(target_width) * u64::from(source_height);
    if source_wider {
        let height = u64::from(source_height) * u64::from(target_width) / u64::from(source_width);
        (target_width, even_floor(height))
    } else {
        let width = u64::from(source_width) * u64::from(target_height) / u64::from(source_height);
        (even_floor(width), target_height)
    }
}

fn even_floor(value: u64) -> u32 {
    u32::try_from(value.max(2) & !1).unwrap_or(u32::MAX - 1)
}

fn resize_filter(
    requested_width: u32,
    requested_height: u32,
    output_width: u32,
    output_height: u32,
    mode: ResizeMode,
    rotation: Rotation,
) -> String {
    let mut filters = Vec::new();
    match rotation {
        Rotation::None => {}
        Rotation::Clockwise90 => filters.push("transpose=clock".to_owned()),
        Rotation::Clockwise180 => filters.extend(["hflip".to_owned(), "vflip".to_owned()]),
        Rotation::CounterClockwise90 => filters.push("transpose=cclock".to_owned()),
    }
    match mode {
        ResizeMode::Stretch | ResizeMode::Fit => {
            filters.push(format!("scale={output_width}:{output_height}"));
        }
        ResizeMode::Crop => {
            filters.push(format!(
                "scale=w={requested_width}:h={requested_height}:force_original_aspect_ratio=increase:force_divisible_by=2"
            ));
            filters.push(format!("crop={requested_width}:{requested_height}"));
        }
        ResizeMode::Pad => {
            filters.push(format!(
                "scale=w={requested_width}:h={requested_height}:force_original_aspect_ratio=decrease:force_divisible_by=2"
            ));
            filters.push(format!(
                "pad={requested_width}:{requested_height}:(ow-iw)/2:(oh-ih)/2"
            ));
        }
    }
    filters.push("setsar=1".to_owned());
    filters.join(",")
}

fn subtitles_filter(path: &Path) -> String {
    let mut escaped = String::new();
    for character in path.display().to_string().chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            ':' => escaped.push_str("\\:"),
            ',' => escaped.push_str("\\,"),
            ';' => escaped.push_str("\\;"),
            '[' => escaped.push_str("\\["),
            ']' => escaped.push_str("\\]"),
            '=' => escaped.push_str("\\="),
            other => escaped.push(other),
        }
    }
    format!("subtitles=filename='{escaped}'")
}

fn verification_checks(expected: &ExpectedOutput) -> Vec<VerificationCheck> {
    let mut checks = vec![
        VerificationCheck::Parseable,
        VerificationCheck::Container {
            name: expected.container.clone(),
        },
    ];
    if let Some(duration_ms) = expected.duration_ms {
        checks.push(VerificationCheck::Duration {
            expected_ms: duration_ms,
            tolerance_ms: expected.duration_tolerance_ms,
        });
    }
    if let Some(video) = &expected.video {
        checks.push(VerificationCheck::Video {
            codec: video.codec.clone(),
            width: video.width,
            height: video.height,
            sample_aspect_ratio: video.sample_aspect_ratio.clone(),
            average_frame_rate: video.average_frame_rate.clone(),
        });
    }
    if let Some(audio) = &expected.audio {
        checks.push(VerificationCheck::Audio {
            codec: audio.codec.clone(),
            channels: audio.channels,
        });
        if let Some(loudness) = &audio.loudness {
            checks.push(VerificationCheck::Loudness {
                integrated_lufs_x100: loudness.integrated_lufs_x100,
                tolerance_lu_x100: loudness.tolerance_lu_x100,
            });
        }
    }
    checks.push(VerificationCheck::StreamLayout {
        video_streams: u32::from(expected.video.is_some()),
        audio_streams: u32::from(expected.audio.is_some()),
        subtitle_streams: 0,
        other_streams: 0,
    });
    checks
}

fn verification_checks_with_size(
    expected: &ExpectedOutput,
    max_output_bytes: u64,
) -> Vec<VerificationCheck> {
    let mut checks = verification_checks(expected);
    checks.push(VerificationCheck::OutputSize {
        max_bytes: max_output_bytes,
    });
    checks.push(VerificationCheck::DistinctFromInputs);
    checks
}

fn dropped_stream_warnings(
    source_index: usize,
    streams: &[StreamSummary],
    selected: &[SelectedStream],
) -> Vec<PlanWarning> {
    let dropped: Vec<String> = streams
        .iter()
        .filter(|stream| {
            !selected.iter().any(|selected| {
                selected.source_index == source_index && selected.stream_index == stream.index
            })
        })
        .map(|stream| format!("{} ({:?})", stream.index, stream.kind))
        .collect();
    let mut warnings = Vec::new();
    if !dropped.is_empty() {
        warnings.push(PlanWarning {
            code: "streams_dropped".to_owned(),
            message: format!(
                "unselected streams from input {source_index} will be dropped: {}",
                dropped.join(", ")
            ),
        });
    }
    warnings.push(PlanWarning {
        code: "metadata_removed".to_owned(),
        message: "input metadata is not copied to the web target".to_owned(),
    });
    warnings
}

fn temporary_output_path(output: &Path, id: &str) -> Result<PathBuf, AvpactError> {
    let name = output.file_name().and_then(OsStr::to_str).ok_or_else(|| {
        AvpactError::OutputPathInvalid {
            path: output.to_path_buf(),
            message: "output file name must be valid UTF-8".to_owned(),
        }
    })?;
    let extension = output.extension().and_then(OsStr::to_str).ok_or_else(|| {
        AvpactError::OutputPathInvalid {
            path: output.to_path_buf(),
            message: "output extension must be valid UTF-8".to_owned(),
        }
    })?;
    Ok(output.with_file_name(format!(".{name}.{id}.tmp.{extension}")))
}

fn clip_argv(
    input: &Path,
    temporary_output: &Path,
    start_ms: u64,
    duration_ms: u64,
    video_index: u32,
    audio_index: Option<u32>,
) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-ss".to_owned(),
        format_milliseconds(start_ms),
        "-t".to_owned(),
        format_milliseconds(duration_ms),
        "-map".to_owned(),
        format!("0:{video_index}"),
    ]);
    if let Some(audio_index) = audio_index {
        argv.extend(["-map".to_owned(), format!("0:{audio_index}")]);
    }
    argv.extend([
        "-map_metadata".to_owned(),
        "-1".to_owned(),
        "-c:v".to_owned(),
        "libx264".to_owned(),
        "-preset".to_owned(),
        "medium".to_owned(),
        "-crf".to_owned(),
        "23".to_owned(),
        "-pix_fmt".to_owned(),
        "yuv420p".to_owned(),
        "-vf".to_owned(),
        "setpts=PTS-STARTPTS,setsar=1".to_owned(),
    ]);
    append_aac_args(&mut argv, audio_index.is_some(), true);
    append_mp4_output(&mut argv, temporary_output);
    argv
}

fn transcode_argv(
    input: &Path,
    temporary_output: &Path,
    video_index: u32,
    audio_index: Option<u32>,
    video_filter: Option<&str>,
) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-map".to_owned(),
        format!("0:{video_index}"),
    ]);
    if let Some(audio_index) = audio_index {
        argv.extend(["-map".to_owned(), format!("0:{audio_index}")]);
    }
    argv.extend([
        "-map_metadata".to_owned(),
        "-1".to_owned(),
        "-c:v".to_owned(),
        "libx264".to_owned(),
        "-preset".to_owned(),
        "medium".to_owned(),
        "-crf".to_owned(),
        "23".to_owned(),
        "-pix_fmt".to_owned(),
        "yuv420p".to_owned(),
    ]);
    let filter = match video_filter {
        Some(filter) if filter.split(',').any(|part| part.starts_with("setsar=")) => {
            filter.to_owned()
        }
        Some(filter) => format!("{filter},setsar=1"),
        None => "setsar=1".to_owned(),
    };
    argv.extend(["-vf".to_owned(), filter]);
    append_aac_args(&mut argv, audio_index.is_some(), false);
    append_mp4_output(&mut argv, temporary_output);
    argv
}

fn extract_audio_argv(input: &Path, temporary_output: &Path, audio_index: u32) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-map".to_owned(),
        format!("0:{audio_index}"),
        "-map_metadata".to_owned(),
        "-1".to_owned(),
        "-vn".to_owned(),
        "-c:a".to_owned(),
        "aac".to_owned(),
        "-b:a".to_owned(),
        "192k".to_owned(),
        "-f".to_owned(),
        "ipod".to_owned(),
        "-n".to_owned(),
        temporary_output.display().to_string(),
    ]);
    argv
}

fn normalize_audio_argv(
    input: &Path,
    temporary_output: &Path,
    audio_index: u32,
    target_lufs_x100: i32,
) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-map".to_owned(),
        format!("0:{audio_index}"),
        "-map_metadata".to_owned(),
        "-1".to_owned(),
        "-vn".to_owned(),
        "-af".to_owned(),
        format!(
            "loudnorm=I={}:TP=-1.0:LRA=11",
            format_loudness(target_lufs_x100)
        ),
        "-ar".to_owned(),
        "48000".to_owned(),
        "-c:a".to_owned(),
        "aac".to_owned(),
        "-b:a".to_owned(),
        "192k".to_owned(),
        "-f".to_owned(),
        "ipod".to_owned(),
        "-n".to_owned(),
        temporary_output.display().to_string(),
    ]);
    argv
}

fn concatenate_argv(
    inputs: &[PathBuf],
    temporary_output: &Path,
    video_indices: &[u32],
    audio_indices: &[Option<u32>],
    has_audio: bool,
) -> Vec<String> {
    let mut argv = backend_prefix();
    for input in inputs {
        argv.extend(["-i".to_owned(), input.display().to_string()]);
    }
    let mut filters = Vec::new();
    for (source_index, video_index) in video_indices.iter().enumerate() {
        filters.push(format!(
            "[{source_index}:{video_index}]setpts=PTS-STARTPTS,setsar=1,format=yuv420p[v{source_index}]"
        ));
        if has_audio {
            let audio_index = audio_indices[source_index].expect("all inputs have audio");
            filters.push(format!(
                "[{source_index}:{audio_index}]asetpts=PTS-STARTPTS,aformat=sample_rates=48000:channel_layouts=stereo[a{source_index}]"
            ));
        }
    }
    let mut concat_inputs = String::new();
    for source_index in 0..inputs.len() {
        concat_inputs.push_str(&format!("[v{source_index}]"));
        if has_audio {
            concat_inputs.push_str(&format!("[a{source_index}]"));
        }
    }
    if has_audio {
        filters.push(format!(
            "{concat_inputs}concat=n={}:v=1:a=1[vout][aout]",
            inputs.len()
        ));
    } else {
        filters.push(format!(
            "{concat_inputs}concat=n={}:v=1:a=0[vout]",
            inputs.len()
        ));
    }
    argv.extend([
        "-filter_complex".to_owned(),
        filters.join(";"),
        "-map".to_owned(),
        "[vout]".to_owned(),
    ]);
    if has_audio {
        argv.extend(["-map".to_owned(), "[aout]".to_owned()]);
    }
    argv.extend([
        "-map_metadata".to_owned(),
        "-1".to_owned(),
        "-c:v".to_owned(),
        "libx264".to_owned(),
        "-preset".to_owned(),
        "medium".to_owned(),
        "-crf".to_owned(),
        "23".to_owned(),
        "-pix_fmt".to_owned(),
        "yuv420p".to_owned(),
    ]);
    append_aac_args(&mut argv, has_audio, false);
    append_mp4_output(&mut argv, temporary_output);
    argv
}

fn thumbnail_argv(
    input: &Path,
    temporary_output: &Path,
    video_index: u32,
    at_ms: u64,
    width: u32,
    height: u32,
) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-ss".to_owned(),
        format_milliseconds(at_ms),
        "-map".to_owned(),
        format!("0:{video_index}"),
        "-frames:v".to_owned(),
        "1".to_owned(),
        "-an".to_owned(),
        "-vf".to_owned(),
        format!("scale={width}:{height},setsar=1"),
        "-c:v".to_owned(),
        "mjpeg".to_owned(),
        "-q:v".to_owned(),
        "2".to_owned(),
        "-f".to_owned(),
        "image2".to_owned(),
        "-n".to_owned(),
        temporary_output.display().to_string(),
    ]);
    argv
}

#[allow(clippy::too_many_arguments)]
fn contact_sheet_argv(
    input: &Path,
    temporary_output: &Path,
    video_index: u32,
    interval_ms: u64,
    columns: u32,
    rows: u32,
    cell_width: u32,
    cell_height: u32,
) -> Vec<String> {
    let mut argv = backend_prefix();
    argv.extend([
        "-i".to_owned(),
        input.display().to_string(),
        "-map".to_owned(),
        format!("0:{video_index}"),
        "-frames:v".to_owned(),
        "1".to_owned(),
        "-an".to_owned(),
        "-vf".to_owned(),
        format!(
            "fps=1000/{interval_ms},scale={cell_width}:{cell_height},setsar=1,tile={columns}x{rows}:nb_frames={}",
            columns.saturating_mul(rows)
        ),
        "-c:v".to_owned(),
        "mjpeg".to_owned(),
        "-q:v".to_owned(),
        "2".to_owned(),
        "-f".to_owned(),
        "image2".to_owned(),
        "-n".to_owned(),
        temporary_output.display().to_string(),
    ]);
    argv
}

fn format_loudness(value_x100: i32) -> String {
    format!(
        "{}.{:02}",
        value_x100 / 100,
        value_x100.unsigned_abs() % 100
    )
}

fn backend_prefix() -> Vec<String> {
    vec![
        "-hide_banner".to_owned(),
        "-nostdin".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-nostats".to_owned(),
        "-stats_period".to_owned(),
        "0.5".to_owned(),
        "-progress".to_owned(),
        "pipe:1".to_owned(),
    ]
}

fn append_aac_args(argv: &mut Vec<String>, has_audio: bool, reset_timestamps: bool) {
    if has_audio {
        argv.extend([
            "-c:a".to_owned(),
            "aac".to_owned(),
            "-b:a".to_owned(),
            "128k".to_owned(),
        ]);
        if reset_timestamps {
            argv.extend(["-af".to_owned(), "asetpts=PTS-STARTPTS".to_owned()]);
        }
    }
}

fn append_mp4_output(argv: &mut Vec<String>, temporary_output: &Path) {
    argv.extend([
        "-movflags".to_owned(),
        "+faststart".to_owned(),
        "-f".to_owned(),
        "mp4".to_owned(),
        "-n".to_owned(),
        temporary_output.display().to_string(),
    ]);
}

fn format_milliseconds(value: u64) -> String {
    format!("{}.{:03}", value / 1000, value % 1000)
}

fn is_audio(stream: &StreamSummary) -> bool {
    stream.kind == StreamKind::Audio
}

fn digest_json<T: Serialize>(value: &T) -> Result<String, AvpactError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(encode_lower(Sha256::digest(bytes)))
}

fn read_bounded_document(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Serialize)]
struct PlanIdentityMaterial<'a> {
    recipe_digest: &'a str,
    constraints_digest: &'a str,
    inputs: &'a [InspectionReport],
    output: &'a Path,
    overwrite: OverwritePolicy,
    operation: &'a PlannedOperation,
    selected_streams: &'a [SelectedStream],
    backend_version: &'a str,
    backend_configuration: &'a str,
    backend_library_versions: &'a BTreeMap<String, String>,
    expected: &'a ExpectedOutput,
    verification_checks: &'a [VerificationCheck],
    warnings: &'a [PlanWarning],
    resources: &'a ResourcePlan,
}

fn expected_plan_id(plan: &Plan) -> Result<String, AvpactError> {
    let material = PlanIdentityMaterial {
        recipe_digest: &plan.recipe_digest,
        constraints_digest: &plan.constraints_digest,
        inputs: &plan.inputs,
        output: &plan.output.path,
        overwrite: plan.output.overwrite,
        operation: &plan.operation,
        selected_streams: &plan.selected_streams,
        backend_version: &plan.backend.version,
        backend_configuration: &plan.backend.configuration,
        backend_library_versions: &plan.backend.library_versions,
        expected: &plan.expected,
        verification_checks: &plan.verification_checks,
        warnings: &plan.warnings,
        resources: &plan.resources,
    };
    Ok(format!("plan_{}", &digest_json(&material)?[..32]))
}

fn selected_kind(
    selected: &[SelectedStream],
    kind: StreamKind,
) -> Result<&SelectedStream, AvpactError> {
    let mut matches = selected.iter().filter(|stream| stream.kind == kind);
    let first = matches.next().ok_or_else(|| AvpactError::PlanInvalid {
        message: format!("plan has no selected {kind:?} stream"),
    })?;
    if matches.next().is_some() {
        return invalid_plan(format!("plan selects more than one {kind:?} stream"));
    }
    Ok(first)
}

fn selected_optional_kind(
    selected: &[SelectedStream],
    kind: StreamKind,
) -> Result<Option<&SelectedStream>, AvpactError> {
    let mut matches = selected.iter().filter(|stream| stream.kind == kind);
    let first = matches.next();
    if matches.next().is_some() {
        return invalid_plan(format!("plan selects more than one {kind:?} stream"));
    }
    Ok(first)
}

fn input_stream(
    inspection: &InspectionReport,
    index: u32,
    kind: StreamKind,
) -> Result<&StreamSummary, AvpactError> {
    inspection
        .streams
        .iter()
        .find(|stream| stream.index == index && stream.kind == kind)
        .ok_or_else(|| AvpactError::PlanInvalid {
            message: format!("selected input stream {index} is not {kind:?}"),
        })
}

fn single_input(plan: &Plan) -> Result<&InspectionReport, AvpactError> {
    if plan.inputs.len() != 1 {
        return invalid_plan("operation requires exactly one input");
    }
    Ok(&plan.inputs[0])
}

fn invalid_plan<T>(message: impl Into<String>) -> Result<T, AvpactError> {
    Err(AvpactError::PlanInvalid {
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::model::{
        AudioSummary, BackendIdentity, FormatSummary, SourceIdentity, VideoSummary,
    };

    use super::*;

    #[test]
    fn milliseconds_have_a_fixed_decimal_representation() {
        assert_eq!(format_milliseconds(0), "0.000");
        assert_eq!(format_milliseconds(12_034), "12.034");
    }

    #[test]
    fn encoder_listing_requires_an_exact_name() {
        let listing = " V..... libx264 H.264\n A..... aac AAC\n";
        assert!(has_encoder(listing, "libx264"));
        assert!(has_encoder(listing, "aac"));
        assert!(!has_encoder(listing, "h264"));
    }

    #[test]
    fn every_example_recipe_matches_the_typed_contract() {
        for (name, contents) in [
            ("clip", include_str!("../examples/clip.recipe.json")),
            (
                "transcode",
                include_str!("../examples/transcode.recipe.json"),
            ),
            ("resize", include_str!("../examples/resize.recipe.json")),
            (
                "extract-audio",
                include_str!("../examples/extract-audio.recipe.json"),
            ),
            (
                "normalize-audio",
                include_str!("../examples/normalize-audio.recipe.json"),
            ),
            (
                "concatenate",
                include_str!("../examples/concatenate.recipe.json"),
            ),
            (
                "thumbnail",
                include_str!("../examples/thumbnail.recipe.json"),
            ),
            (
                "contact-sheet",
                include_str!("../examples/contact-sheet.recipe.json"),
            ),
            (
                "burn-subtitles",
                include_str!("../examples/burn-subtitles.recipe.json"),
            ),
        ] {
            let recipe: Recipe = serde_json::from_str(contents)
                .unwrap_or_else(|error| panic!("{name} example: {error}"));
            validate_recipe(&recipe).unwrap_or_else(|error| panic!("{name} example: {error}"));
        }
    }

    #[test]
    fn clip_plan_is_deterministic_and_explicit() {
        let input = fixture_inspection();
        let recipe = Recipe {
            schema_version: crate::RECIPE_SCHEMA_VERSION.to_owned(),
            operation: Operation::Clip {
                input: "source.mp4".into(),
                output: "clip.mp4".into(),
                start_ms: 250,
                end_ms: 1_250,
            },
            target: Target::Web,
            constraints: RecipeConstraints::default(),
        };
        let inputs = PlanInputs {
            recipe: recipe.clone(),
            recipe_digest: digest_json(&recipe).expect("recipe digest"),
            constraints_digest: digest_json(&recipe.constraints).expect("constraints digest"),
            inputs: vec![input],
            output: PathBuf::from("/fixture/clip.mp4"),
            backend: fixture_backend_identity(),
        };

        let first = compile_clip(inputs).expect("compile plan");
        let second = compile_clip(PlanInputs {
            recipe_digest: digest_json(&recipe).expect("recipe digest"),
            constraints_digest: digest_json(&recipe.constraints).expect("constraints digest"),
            recipe,
            inputs: vec![fixture_inspection()],
            output: PathBuf::from("/fixture/clip.mp4"),
            backend: fixture_backend_identity(),
        })
        .expect("compile plan");

        assert_eq!(first.id, second.id);
        assert_eq!(first.backend.argv, second.backend.argv);
        assert_eq!(first.selected_streams.len(), 2);
        assert_eq!(
            validate_plan(&first).expect("validate generated plan"),
            first.backend.argv
        );
        let expected_temporary_path =
            PathBuf::from("/fixture").join(format!(".clip.mp4.{}.tmp.mp4", first.id));
        assert_eq!(first.output.temporary_path, expected_temporary_path);
        let expected_temporary_argument = expected_temporary_path.display().to_string();
        assert_eq!(
            first.backend.argv,
            vec![
                "-hide_banner",
                "-nostdin",
                "-loglevel",
                "error",
                "-nostats",
                "-stats_period",
                "0.5",
                "-progress",
                "pipe:1",
                "-i",
                "/fixture/source.mp4",
                "-ss",
                "0.250",
                "-t",
                "1.000",
                "-map",
                "0:0",
                "-map",
                "0:1",
                "-map_metadata",
                "-1",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-vf",
                "setpts=PTS-STARTPTS,setsar=1",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-af",
                "asetpts=PTS-STARTPTS",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
                "-n",
                expected_temporary_argument.as_str(),
            ]
        );

        let mut inspection_tampered = first.clone();
        inspection_tampered.inputs[0].format.long_name = Some("tampered".to_owned());
        assert!(matches!(
            validate_plan(&inspection_tampered),
            Err(AvpactError::PlanInvalid { .. })
        ));

        let mut tampered = first;
        tampered.backend.argv.push("-arbitrary-option".to_owned());
        assert!(matches!(
            validate_plan(&tampered),
            Err(AvpactError::PlanInvalid { .. })
        ));
    }

    fn fixture_backend_identity() -> FfmpegBuildIdentity {
        FfmpegBuildIdentity {
            version: "ffmpeg version fixture".to_owned(),
            configuration: "--enable-fixture".to_owned(),
            library_versions: BTreeMap::from([(
                "libavcodec".to_owned(),
                "1.2.3 / 1.2.3".to_owned(),
            )]),
        }
    }

    fn fixture_inspection() -> InspectionReport {
        InspectionReport {
            schema_version: crate::INSPECTION_SCHEMA_VERSION.to_owned(),
            source: SourceIdentity {
                path: PathBuf::from("/fixture/source.mp4"),
                size_bytes: 10,
                sha256: "a".repeat(64),
            },
            backend: BackendIdentity {
                name: "ffprobe".to_owned(),
                version: "ffprobe version fixture".to_owned(),
            },
            format: FormatSummary {
                name: Some("mov,mp4,m4a,3gp,3g2,mj2".to_owned()),
                long_name: Some("QuickTime / MOV".to_owned()),
                duration_ms: Some(2_000),
                bit_rate_bps: Some(1_000_000),
                tags: None,
            },
            streams: vec![
                StreamSummary {
                    index: 0,
                    kind: StreamKind::Video,
                    codec: Some("h264".to_owned()),
                    profile: Some("High".to_owned()),
                    duration_ms: Some(2_000),
                    bit_rate_bps: Some(800_000),
                    video: Some(VideoSummary {
                        width: Some(1920),
                        height: Some(1080),
                        pixel_format: Some("yuv420p".to_owned()),
                        average_frame_rate: Some("30/1".to_owned()),
                        sample_aspect_ratio: Some("1:1".to_owned()),
                        display_aspect_ratio: Some("16:9".to_owned()),
                    }),
                    audio: None,
                    disposition: BTreeMap::from([("default".to_owned(), 1)]),
                    tags: None,
                },
                StreamSummary {
                    index: 1,
                    kind: StreamKind::Audio,
                    codec: Some("aac".to_owned()),
                    profile: Some("LC".to_owned()),
                    duration_ms: Some(2_000),
                    bit_rate_bps: Some(128_000),
                    video: None,
                    audio: Some(AudioSummary {
                        sample_rate_hz: Some(48_000),
                        channels: Some(2),
                        channel_layout: Some("stereo".to_owned()),
                        sample_format: Some("fltp".to_owned()),
                    }),
                    disposition: BTreeMap::from([("default".to_owned(), 1)]),
                    tags: None,
                },
            ],
        }
    }
}
