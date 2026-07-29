use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AvpactError, ReceiptRecovery, bounded_diagnostic};
use crate::hex::encode_lower;
use crate::inspect;
use crate::model::{InspectionReport, StreamKind};
use crate::plan::{self, Plan, PlanWarning, PlannedBackend};

const MAX_BACKEND_DIAGNOSTIC_BYTES: usize = 8 * 1024;
const MAX_PROGRESS_LINE_BYTES: usize = 1_024;
const MAX_PROGRESS_TEXT_CHARACTERS: usize = 64;
const MAX_RECEIPT_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(unix)]
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(2);

struct ExecutionLimits<'a> {
    temporary_path: &'a Path,
    max_output_bytes: u64,
    max_temporary_bytes: u64,
    max_runtime_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ProgressEvent {
    pub schema_version: String,
    pub plan_id: String,
    pub sequence: u64,
    pub state: ProgressState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    Running,
    Finished,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub schema_version: String,
    pub plan_id: String,
    pub passed: bool,
    pub output: InspectionReport,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub check: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: String,
    pub id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub started_unix_ms: u64,
    pub completed_unix_ms: u64,
    pub elapsed_ms: u64,
    pub backend: PlannedBackend,
    pub warnings: Vec<PlanWarning>,
    pub verification: VerificationReport,
    pub publication: Publication,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Publication {
    pub output: PathBuf,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_warning: Option<String>,
}

pub fn apply_plan<F>(
    plan_path: &Path,
    receipt_path: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
    on_progress: F,
) -> Result<Receipt, AvpactError>
where
    F: FnMut(&ProgressEvent),
{
    apply_plan_with_cancellation(
        plan_path,
        receipt_path,
        ffmpeg,
        ffprobe,
        &CancellationToken::new(),
        on_progress,
    )
}

pub fn apply_plan_with_cancellation<F>(
    plan_path: &Path,
    receipt_path: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
    cancellation: &CancellationToken,
    on_progress: F,
) -> Result<Receipt, AvpactError>
where
    F: FnMut(&ProgressEvent),
{
    preflight_receipt_path(receipt_path)?;
    let receipt =
        execute_plan_with_cancellation(plan_path, ffmpeg, ffprobe, cancellation, on_progress)?;
    persist_receipt_with_recovery(receipt_path, &receipt)?;
    Ok(receipt)
}

pub fn apply_plan_to_store_with_cancellation<F>(
    plan_path: &Path,
    state_dir: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
    cancellation: &CancellationToken,
    on_progress: F,
) -> Result<Receipt, AvpactError>
where
    F: FnMut(&ProgressEvent),
{
    ensure_receipt_store(state_dir)?;
    let receipt =
        execute_plan_with_cancellation(plan_path, ffmpeg, ffprobe, cancellation, on_progress)?;
    let receipt_path = receipt_store_path(state_dir, &receipt.id)?;
    persist_receipt_with_recovery(&receipt_path, &receipt)?;
    Ok(receipt)
}

fn execute_plan_with_cancellation<F>(
    plan_path: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
    cancellation: &CancellationToken,
    mut on_progress: F,
) -> Result<Receipt, AvpactError>
where
    F: FnMut(&ProgressEvent),
{
    let plan = plan::read_plan(plan_path)?;
    let argv = plan::validate_plan(&plan)?;
    preflight_output(&plan)?;

    for planned_input in &plan.inputs {
        let current_source = inspect::identify_source(&planned_input.source.path)?;
        if current_source != planned_input.source {
            return Err(AvpactError::InputChanged {
                path: planned_input.source.path.clone(),
            });
        }
    }
    let actual_probe_version = inspect::probe_version(ffprobe)?;
    if plan
        .inputs
        .iter()
        .any(|input| input.backend.version != actual_probe_version)
    {
        let planned_probe_versions = plan
            .inputs
            .iter()
            .map(|input| input.backend.version.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        return Err(AvpactError::BackendIdentityMismatch {
            planned: format!("ffprobe versions {planned_probe_versions:?}"),
            actual: format!("ffprobe version {actual_probe_version:?}"),
        });
    }
    let actual_backend = plan::backend_identity(ffmpeg)?;
    if actual_backend.version != plan.backend.version
        || actual_backend.configuration != plan.backend.configuration
        || actual_backend.library_versions != plan.backend.library_versions
    {
        return Err(AvpactError::BackendIdentityMismatch {
            planned: backend_identity_summary(
                &plan.backend.version,
                &plan.backend.configuration,
                &plan.backend.library_versions,
            ),
            actual: backend_identity_summary(
                &actual_backend.version,
                &actual_backend.configuration,
                &actual_backend.library_versions,
            ),
        });
    }

    let started_unix_ms = unix_time_ms();
    let started = Instant::now();
    if let Err(error) = execute_backend(
        ffmpeg,
        &argv,
        &plan.id,
        cancellation,
        Some(ExecutionLimits {
            temporary_path: &plan.output.temporary_path,
            max_output_bytes: plan.resources.max_output_bytes,
            max_temporary_bytes: plan.resources.max_temporary_bytes,
            max_runtime_ms: plan.resources.max_runtime_ms,
        }),
        &mut on_progress,
    ) {
        remove_generated_temporary(&plan.output.temporary_path);
        return Err(error);
    }

    let mut verification = match verify_output(&plan.output.temporary_path, &plan, ffmpeg, ffprobe)
    {
        Ok(verification) => verification,
        Err(error) => {
            remove_generated_temporary(&plan.output.temporary_path);
            return Err(error);
        }
    };
    if !verification.passed {
        let summary = verification
            .checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.check.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        remove_generated_temporary(&plan.output.temporary_path);
        return Err(AvpactError::VerificationFailed {
            path: plan.output.temporary_path.clone(),
            summary,
        });
    }

    let cleanup_warning = publish_no_clobber(&plan)?;
    verification.output.source.path = plan.output.path.clone();
    let completed_unix_ms = unix_time_ms();
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let plan_digest = plan::plan_digest(&plan)?;
    let id = receipt_id(
        &plan.id,
        &verification.output.source.sha256,
        completed_unix_ms,
    );
    let receipt = Receipt {
        schema_version: crate::RECEIPT_SCHEMA_VERSION.to_owned(),
        id,
        plan_id: plan.id,
        plan_digest,
        started_unix_ms,
        completed_unix_ms,
        elapsed_ms,
        backend: plan.backend,
        warnings: plan.warnings,
        verification,
        publication: Publication {
            output: plan.output.path,
            method: "same_filesystem_hard_link".to_owned(),
            cleanup_warning,
        },
    };
    Ok(receipt)
}

fn backend_identity_summary(
    version: &str,
    configuration: &str,
    library_versions: &BTreeMap<String, String>,
) -> String {
    format!("{version:?}, configuration {configuration:?}, libraries {library_versions:?}")
}

pub fn verify_output(
    output_path: &Path,
    plan: &Plan,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<VerificationReport, AvpactError> {
    plan::validate_plan(plan)?;
    let output = inspect::inspect(output_path, ffprobe)?;
    if plan
        .inputs
        .iter()
        .any(|input| input.backend.version != output.backend.version)
    {
        let planned_probe_versions = plan
            .inputs
            .iter()
            .map(|input| input.backend.version.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        return Err(AvpactError::BackendIdentityMismatch {
            planned: format!("ffprobe versions {planned_probe_versions:?}"),
            actual: format!("ffprobe version {:?}", output.backend.version),
        });
    }
    let mut checks = Vec::new();
    checks.push(check("parseable", true, "true", "true"));
    checks.push(check(
        "container",
        output.format.name.as_deref() == Some(&plan.expected.container),
        &plan.expected.container,
        output.format.name.as_deref().unwrap_or("unknown"),
    ));

    if let Some(expected_duration) = plan.expected.duration_ms {
        let actual_duration = output.format.duration_ms;
        checks.push(check(
            "duration",
            actual_duration.is_some_and(|actual| {
                actual.abs_diff(expected_duration) <= plan.expected.duration_tolerance_ms
            }),
            format!(
                "{}ms ±{}ms",
                expected_duration, plan.expected.duration_tolerance_ms
            ),
            actual_duration
                .map(|duration| format!("{duration}ms"))
                .unwrap_or_else(|| "unknown".to_owned()),
        ));
    }

    if let Some(expected_video) = &plan.expected.video {
        let video = output
            .streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Video);
        let actual_video = video.and_then(|stream| {
            stream.video.as_ref().map(|video| {
                (
                    stream.codec.as_deref(),
                    video.width,
                    video.height,
                    video.sample_aspect_ratio.as_deref(),
                    video.average_frame_rate.as_deref(),
                )
            })
        });
        let video_passed =
            actual_video.is_some_and(|(codec, width, height, aspect, frame_rate)| {
                codec == Some(expected_video.codec.as_str())
                    && width == Some(expected_video.width)
                    && height == Some(expected_video.height)
                    && aspect == Some(expected_video.sample_aspect_ratio.as_str())
                    && expected_video
                        .average_frame_rate
                        .as_deref()
                        .is_none_or(|expected| frame_rate == Some(expected))
            });
        checks.push(check(
            "video",
            video_passed,
            format!(
                "{} {}x{} SAR {} FPS {}",
                expected_video.codec,
                expected_video.width,
                expected_video.height,
                expected_video.sample_aspect_ratio,
                expected_video
                    .average_frame_rate
                    .as_deref()
                    .unwrap_or("any")
            ),
            actual_video
                .map(|(codec, width, height, aspect, frame_rate)| {
                    format!(
                        "{} {}x{} SAR {} FPS {}",
                        codec.unwrap_or("unknown"),
                        width
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "?".to_owned()),
                        height
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "?".to_owned()),
                        aspect.unwrap_or("unknown"),
                        frame_rate.unwrap_or("unknown")
                    )
                })
                .unwrap_or_else(|| "missing".to_owned()),
        ));
    }

    if let Some(expected_audio) = &plan.expected.audio {
        let audio = output
            .streams
            .iter()
            .find(|stream| stream.kind == StreamKind::Audio);
        let actual_audio = audio.map(|stream| {
            (
                stream.codec.as_deref(),
                stream.audio.as_ref().and_then(|audio| audio.channels),
            )
        });
        let audio_passed = actual_audio.is_some_and(|(codec, channels)| {
            codec == Some(expected_audio.codec.as_str())
                && expected_audio
                    .channels
                    .is_none_or(|expected| channels == Some(expected))
        });
        checks.push(check(
            "audio",
            audio_passed,
            format!(
                "{} {}",
                expected_audio.codec,
                expected_audio
                    .channels
                    .map(|channels| format!("{channels}ch"))
                    .unwrap_or_else(|| "any channels".to_owned())
            ),
            actual_audio
                .map(|(codec, channels)| {
                    format!(
                        "{} {}",
                        codec.unwrap_or("unknown"),
                        channels
                            .map(|value| format!("{value}ch"))
                            .unwrap_or_else(|| "unknown channels".to_owned())
                    )
                })
                .unwrap_or_else(|| "missing".to_owned()),
        ));
        if let Some(expected_loudness) = &expected_audio.loudness {
            let actual_lufs_x100 = measure_integrated_loudness(
                ffmpeg,
                output_path,
                expected_loudness.integrated_lufs_x100,
            )?;
            checks.push(check(
                "integrated_loudness",
                actual_lufs_x100.abs_diff(expected_loudness.integrated_lufs_x100)
                    <= expected_loudness.tolerance_lu_x100,
                format!(
                    "{} LUFS ±{} LU",
                    format_x100(expected_loudness.integrated_lufs_x100),
                    format_x100_unsigned(expected_loudness.tolerance_lu_x100)
                ),
                format!("{} LUFS", format_x100(actual_lufs_x100)),
            ));
        }
    }

    let stream_counts = output
        .streams
        .iter()
        .fold([0_u32; 4], |mut counts, stream| {
            let slot = match stream.kind {
                StreamKind::Video => 0,
                StreamKind::Audio => 1,
                StreamKind::Subtitle => 2,
                StreamKind::Data | StreamKind::Attachment | StreamKind::Unknown => 3,
            };
            counts[slot] = counts[slot].saturating_add(1);
            counts
        });
    let expected_stream_counts = [
        u32::from(plan.expected.video.is_some()),
        u32::from(plan.expected.audio.is_some()),
        0,
        0,
    ];
    checks.push(check(
        "stream_layout",
        stream_counts == expected_stream_counts,
        format!(
            "video={} audio={} subtitle=0 other=0",
            expected_stream_counts[0], expected_stream_counts[1]
        ),
        format!(
            "video={} audio={} subtitle={} other={}",
            stream_counts[0], stream_counts[1], stream_counts[2], stream_counts[3]
        ),
    ));

    checks.push(check(
        "output_size",
        output.source.size_bytes <= plan.resources.max_output_bytes,
        format!("<= {} bytes", plan.resources.max_output_bytes),
        format!("{} bytes", output.source.size_bytes),
    ));

    checks.push(check(
        "distinct_from_inputs",
        plan.inputs
            .iter()
            .all(|input| output.source.sha256 != input.source.sha256),
        "different SHA-256 digest",
        if plan
            .inputs
            .iter()
            .any(|input| output.source.sha256 == input.source.sha256)
        {
            "same SHA-256 digest"
        } else {
            "different SHA-256 digest"
        },
    ));
    let passed = checks.iter().all(|check| check.passed);

    Ok(VerificationReport {
        schema_version: crate::VERIFICATION_SCHEMA_VERSION.to_owned(),
        plan_id: plan.id.clone(),
        passed,
        output,
        checks,
    })
}

fn preflight_output(plan: &Plan) -> Result<(), AvpactError> {
    if plan.output.path.exists() {
        return Err(AvpactError::OutputExists {
            path: plan.output.path.clone(),
        });
    }
    if plan.output.temporary_path.exists() {
        return Err(AvpactError::TemporaryOutputExists {
            path: plan.output.temporary_path.clone(),
        });
    }
    let output_directory = plan.output.path.parent().unwrap_or_else(|| Path::new("."));
    let available =
        fs2::available_space(output_directory).map_err(|source| AvpactError::ResourceCheck {
            path: output_directory.to_path_buf(),
            source,
        })?;
    if available < plan.resources.estimated_temporary_bytes {
        return Err(AvpactError::ResourceLimitExceeded {
            resource: "available_temporary_disk_bytes".to_owned(),
            limit: available,
            actual: plan.resources.estimated_temporary_bytes,
        });
    }
    Ok(())
}

fn preflight_receipt_path(receipt_path: &Path) -> Result<(), AvpactError> {
    if receipt_path.exists() {
        return Err(AvpactError::ReceiptExists {
            path: receipt_path.to_path_buf(),
        });
    }
    let parent = receipt_path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        let source = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "receipt output directory does not exist",
        );
        return Err(AvpactError::ReceiptWrite {
            path: receipt_path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn execute_backend<F>(
    ffmpeg: &Path,
    argv: &[String],
    plan_id: &str,
    cancellation: &CancellationToken,
    limits: Option<ExecutionLimits<'_>>,
    on_progress: &mut F,
) -> Result<(), AvpactError>
where
    F: FnMut(&ProgressEvent),
{
    let execution_started = Instant::now();
    let mut command = Command::new(ffmpeg);
    command
        .args(argv)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|source| AvpactError::BackendExecution {
            backend: ffmpeg.display().to_string(),
            source,
        })?;
    let stderr = child.stderr.take().expect("stderr is piped");
    let stderr_reader = thread::spawn(move || read_bounded_tail(stderr));
    let stdout = child.stdout.take().expect("stdout is piped");
    let (progress_sender, progress_receiver) = mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut fields = BTreeMap::new();
        loop {
            let line = match read_bounded_line(&mut reader, MAX_PROGRESS_LINE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => return,
                Err(error) => {
                    let _ = progress_sender.send(Err(error));
                    return;
                }
            };
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if matches!(
                key,
                "frame" | "out_time_us" | "total_size" | "fps" | "speed" | "progress"
            ) {
                fields.insert(key.to_owned(), value.to_owned());
            }
            if key == "progress" {
                if progress_sender.send(Ok(fields)).is_err() {
                    return;
                }
                fields = BTreeMap::new();
            }
        }
    });
    let mut sequence = 0_u64;

    let status = loop {
        if cancellation.is_cancelled() {
            terminate_process_tree(&mut child).map_err(|source| AvpactError::BackendExecution {
                backend: ffmpeg.display().to_string(),
                source,
            })?;
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(AvpactError::Cancelled {
                plan_id: plan_id.to_owned(),
            });
        }
        if let Some(limits) = &limits {
            let elapsed_ms =
                u64::try_from(execution_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            if elapsed_ms > limits.max_runtime_ms {
                terminate_process_tree(&mut child).map_err(|source| {
                    AvpactError::BackendExecution {
                        backend: ffmpeg.display().to_string(),
                        source,
                    }
                })?;
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(AvpactError::ResourceLimitExceeded {
                    resource: "runtime_ms".to_owned(),
                    limit: limits.max_runtime_ms,
                    actual: elapsed_ms,
                });
            }
            if let Ok(metadata) = fs::metadata(limits.temporary_path) {
                let size = metadata.len();
                let limit = limits.max_output_bytes.min(limits.max_temporary_bytes);
                if size > limit {
                    terminate_process_tree(&mut child).map_err(|source| {
                        AvpactError::BackendExecution {
                            backend: ffmpeg.display().to_string(),
                            source,
                        }
                    })?;
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(AvpactError::ResourceLimitExceeded {
                        resource: "output_bytes".to_owned(),
                        limit,
                        actual: size,
                    });
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|source| AvpactError::BackendExecution {
                backend: ffmpeg.display().to_string(),
                source,
            })?
        {
            break status;
        }
        match progress_receiver.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(Ok(fields)) => {
                sequence = sequence.saturating_add(1);
                on_progress(&progress_event(plan_id, sequence, &fields));
            }
            Ok(Err(source)) => {
                terminate_process_tree(&mut child).map_err(|termination_error| {
                    AvpactError::BackendExecution {
                        backend: ffmpeg.display().to_string(),
                        source: termination_error,
                    }
                })?;
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(AvpactError::BackendExecution {
                    backend: ffmpeg.display().to_string(),
                    source,
                });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                thread::sleep(CANCELLATION_POLL_INTERVAL);
            }
        }
    };

    while let Ok(result) = progress_receiver.try_recv() {
        let fields = result.map_err(|source| AvpactError::BackendExecution {
            backend: ffmpeg.display().to_string(),
            source,
        })?;
        sequence = sequence.saturating_add(1);
        on_progress(&progress_event(plan_id, sequence, &fields));
    }
    let _ = stdout_reader.join();
    let diagnostic = stderr_reader
        .join()
        .unwrap_or_else(|_| b"failed to collect backend diagnostic".to_vec());
    if !status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffmpeg.display().to_string(),
            exit_code: status.code(),
            diagnostic: bounded_diagnostic(&diagnostic),
        });
    }
    Ok(())
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
fn configure_process_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    let process_group = -(i32::try_from(child.id()).unwrap_or(i32::MAX));
    let term_result = unsafe { libc::kill(process_group, libc::SIGTERM) };
    if term_result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    let deadline = Instant::now() + TERMINATION_GRACE_PERIOD;
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        thread::sleep(CANCELLATION_POLL_INTERVAL);
    }
    let kill_result = unsafe { libc::kill(process_group, libc::SIGKILL) };
    if kill_result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ESRCH) {
            return Err(error);
        }
    }
    child.wait().map(|_| ())
}

#[cfg(windows)]
fn terminate_process_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    let result = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status()?;
    if !result.success() && child.try_wait()?.is_none() {
        child.kill()?;
    }
    child.wait().map(|_| ())
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(child: &mut std::process::Child) -> std::io::Result<()> {
    child.kill()?;
    child.wait().map(|_| ())
}

fn progress_event(
    plan_id: &str,
    sequence: u64,
    fields: &BTreeMap<String, String>,
) -> ProgressEvent {
    ProgressEvent {
        schema_version: crate::PROGRESS_SCHEMA_VERSION.to_owned(),
        plan_id: plan_id.to_owned(),
        sequence,
        state: if fields.get("progress").is_some_and(|value| value == "end") {
            ProgressState::Finished
        } else {
            ProgressState::Running
        },
        frame: parse_field(fields, "frame"),
        out_time_ms: parse_field::<u64>(fields, "out_time_us").map(|value| value / 1000),
        total_size_bytes: parse_field(fields, "total_size"),
        fps: fields.get("fps").map(|value| bounded_progress_text(value)),
        speed: fields
            .get("speed")
            .map(|value| bounded_progress_text(value)),
    }
}

fn parse_field<T: std::str::FromStr>(fields: &BTreeMap<String, String>, key: &str) -> Option<T> {
    fields.get(key)?.parse().ok()
}

fn read_bounded_tail(mut reader: impl Read) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        result.extend_from_slice(&chunk[..count]);
        if result.len() > MAX_BACKEND_DIAGNOSTIC_BYTES {
            let excess = result.len() - MAX_BACKEND_DIAGNOSTIC_BYTES;
            result.drain(..excess);
        }
    }
    result
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> std::io::Result<Option<String>> {
    let mut line = Vec::with_capacity(max_bytes.min(256));
    let mut saw_input = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(saw_input.then(|| String::from_utf8_lossy(&line).into_owned()));
        }
        saw_input = true;
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        let content_end = if available[..consumed].ends_with(b"\n") {
            consumed - 1
        } else {
            consumed
        };
        let remaining = max_bytes.saturating_sub(line.len());
        line.extend_from_slice(&available[..content_end.min(remaining)]);
        let completed = consumed <= available.len() && available[consumed - 1] == b'\n';
        reader.consume(consumed);
        if completed {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
        }
    }
}

fn bounded_progress_text(value: &str) -> String {
    value.chars().take(MAX_PROGRESS_TEXT_CHARACTERS).collect()
}

fn measure_integrated_loudness(
    ffmpeg: &Path,
    output_path: &Path,
    target_lufs_x100: i32,
) -> Result<i32, AvpactError> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostats", "-i"])
        .arg(output_path)
        .args([
            "-vn",
            "-af",
            &format!(
                "loudnorm=I={}:TP=-1.0:LRA=11:print_format=json",
                format_x100(target_lufs_x100)
            ),
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|source| AvpactError::BackendExecution {
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
    parse_input_loudness(&String::from_utf8_lossy(&output.stderr)).ok_or_else(|| {
        AvpactError::VerificationMeasurement {
            message: "FFmpeg loudnorm output did not contain a finite input_i value".to_owned(),
        }
    })
}

fn parse_input_loudness(diagnostic: &str) -> Option<i32> {
    let key_index = diagnostic.rfind("\"input_i\"")?;
    let after_key = &diagnostic[key_index + "\"input_i\"".len()..];
    let value = after_key.split_once(':')?.1.trim_start();
    let value = value.strip_prefix('"')?.split_once('"')?.0;
    let parsed: f64 = value.parse().ok()?;
    if !parsed.is_finite() || parsed < f64::from(i32::MIN) / 100.0 {
        return None;
    }
    let scaled = (parsed * 100.0).round();
    if scaled > f64::from(i32::MAX) {
        return None;
    }
    Some(scaled as i32)
}

fn format_x100(value: i32) -> String {
    format!("{}.{:02}", value / 100, value.unsigned_abs() % 100)
}

fn format_x100_unsigned(value: u32) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

fn publish_no_clobber(plan: &Plan) -> Result<Option<String>, AvpactError> {
    fs::hard_link(&plan.output.temporary_path, &plan.output.path).map_err(|source| {
        AvpactError::PublishFailed {
            path: plan.output.path.clone(),
            source,
        }
    })?;
    Ok(fs::remove_file(&plan.output.temporary_path)
        .err()
        .map(|error| {
            format!(
                "published output, but could not remove temporary link {}: {error}",
                plan.output.temporary_path.display()
            )
        }))
}

fn remove_generated_temporary(path: &Path) {
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}

pub fn write_new_receipt(path: &Path, receipt: &Receipt) -> Result<(), AvpactError> {
    let json = serde_json::to_vec_pretty(receipt)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|source| AvpactError::ReceiptWrite {
            path: path.to_path_buf(),
            source,
        })?;
    let write_result = file
        .write_all(&json)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all());
    if let Err(source) = write_result {
        return Err(AvpactError::ReceiptWrite {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn persist_receipt_with_recovery(
    requested_path: &Path,
    receipt: &Receipt,
) -> Result<(), AvpactError> {
    persist_receipt_with_recovery_using(requested_path, receipt, write_new_receipt)
}

fn persist_receipt_with_recovery_using<F>(
    requested_path: &Path,
    receipt: &Receipt,
    mut write_receipt: F,
) -> Result<(), AvpactError>
where
    F: FnMut(&Path, &Receipt) -> Result<(), AvpactError>,
{
    let Err(primary_error) = write_receipt(requested_path, receipt) else {
        return Ok(());
    };
    let recovery_path = recovery_receipt_path(receipt);
    let common = (
        receipt.publication.output.clone(),
        receipt.verification.output.source.sha256.clone(),
        requested_path.to_path_buf(),
        recovery_path.clone(),
    );
    match write_receipt(&recovery_path, receipt) {
        Ok(()) => Err(AvpactError::ReceiptRecoveryRequired(Box::new(
            ReceiptRecovery {
                output: common.0,
                output_sha256: common.1,
                requested_receipt: common.2,
                recovery_receipt: common.3,
                recovery_receipt_persisted: true,
                message: primary_error.to_string(),
            },
        ))),
        Err(recovery_error) => Err(AvpactError::ReceiptRecoveryFailed(Box::new(
            ReceiptRecovery {
                output: common.0,
                output_sha256: common.1,
                requested_receipt: common.2,
                recovery_receipt: common.3,
                recovery_receipt_persisted: false,
                message: format!("{primary_error}; recovery persistence failed: {recovery_error}"),
            },
        ))),
    }
}

fn recovery_receipt_path(receipt: &Receipt) -> PathBuf {
    let parent = receipt
        .publication
        .output
        .parent()
        .unwrap_or_else(|| Path::new("."));
    parent.join(format!(".avpact-recovery-{}.json", receipt.id))
}

pub fn receipt_store_path(state_dir: &Path, receipt_id: &str) -> Result<PathBuf, AvpactError> {
    validate_receipt_id(receipt_id)?;
    Ok(state_dir
        .join("receipts")
        .join(format!("{receipt_id}.json")))
}

pub fn read_stored_receipt(state_dir: &Path, receipt_id: &str) -> Result<Receipt, AvpactError> {
    let path = receipt_store_path(state_dir, receipt_id)?;
    read_receipt(&path)
}

pub fn read_receipt(path: &Path) -> Result<Receipt, AvpactError> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_RECEIPT_DOCUMENT_BYTES.saturating_add(1))
                .read_to_end(&mut bytes)
        })
        .map_err(|source| AvpactError::ReceiptRead {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_RECEIPT_DOCUMENT_BYTES {
        return Err(AvpactError::ReceiptInvalid {
            message: format!(
                "receipt exceeds the {} byte document limit",
                MAX_RECEIPT_DOCUMENT_BYTES
            ),
        });
    }
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|source| AvpactError::ReceiptInvalid {
            message: source.to_string(),
        })?;
    validate_receipt(&receipt)?;
    Ok(receipt)
}

fn ensure_receipt_store(state_dir: &Path) -> Result<(), AvpactError> {
    let receipts = state_dir.join("receipts");
    fs::create_dir_all(&receipts).map_err(|source| AvpactError::ReceiptWrite {
        path: receipts,
        source,
    })
}

fn validate_receipt(receipt: &Receipt) -> Result<(), AvpactError> {
    const MAX_RECEIPT_WARNINGS: usize = 128;
    const MAX_WARNING_CHARACTERS: usize = 2_048;
    const MAX_CHECK_CHARACTERS: usize = 4_096;
    if receipt.schema_version != crate::RECEIPT_SCHEMA_VERSION {
        return Err(AvpactError::ReceiptInvalid {
            message: format!(
                "unsupported schema_version {:?}; expected {:?}",
                receipt.schema_version,
                crate::RECEIPT_SCHEMA_VERSION
            ),
        });
    }
    validate_receipt_id(&receipt.id)?;
    if receipt
        .plan_id
        .strip_prefix("plan_")
        .is_none_or(|digest| !is_lower_hex(digest, 32))
    {
        return Err(AvpactError::ReceiptInvalid {
            message: "plan_id must contain plan_ and 32 lowercase hexadecimal characters"
                .to_owned(),
        });
    }
    if !is_lower_hex(&receipt.plan_digest, 64) {
        return Err(AvpactError::ReceiptInvalid {
            message: "plan_digest must be a 64-character lowercase SHA-256 digest".to_owned(),
        });
    }
    if !receipt.verification.passed {
        return Err(AvpactError::ReceiptInvalid {
            message: "successful receipt must contain a passing verification report".to_owned(),
        });
    }
    if receipt.verification.schema_version != crate::VERIFICATION_SCHEMA_VERSION
        || receipt.verification.plan_id != receipt.plan_id
        || receipt
            .verification
            .checks
            .iter()
            .any(|check| !check.passed)
    {
        return Err(AvpactError::ReceiptInvalid {
            message: "verification report is inconsistent with the receipt".to_owned(),
        });
    }
    if receipt.warnings.len() > MAX_RECEIPT_WARNINGS {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt contains too many warnings".to_owned(),
        });
    }
    if receipt.warnings.iter().any(|warning| {
        warning.code.len() > 64 || warning.message.chars().count() > MAX_WARNING_CHARACTERS
    }) {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt warning fields exceed their bounds".to_owned(),
        });
    }
    if receipt.verification.checks.iter().any(|check| {
        check.check.len() > 64
            || check.expected.chars().count() > MAX_CHECK_CHARACTERS
            || check.actual.chars().count() > MAX_CHECK_CHARACTERS
    }) {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt verification fields exceed their bounds".to_owned(),
        });
    }
    if receipt.backend.name != "ffmpeg"
        || receipt.backend.version.is_empty()
        || receipt.backend.library_versions.is_empty()
    {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt backend identity is incomplete".to_owned(),
        });
    }
    if receipt.started_unix_ms > receipt.completed_unix_ms {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt timing is inconsistent".to_owned(),
        });
    }
    if !is_lower_hex(&receipt.verification.output.source.sha256, 64) {
        return Err(AvpactError::ReceiptInvalid {
            message: "verified output digest must be a lowercase SHA-256 value".to_owned(),
        });
    }
    if receipt.publication.output != receipt.verification.output.source.path {
        return Err(AvpactError::ReceiptInvalid {
            message: "publication output and verified output path differ".to_owned(),
        });
    }
    if receipt.publication.method != "same_filesystem_hard_link" {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt publication method is unsupported".to_owned(),
        });
    }
    let expected_id = receipt_id(
        &receipt.plan_id,
        &receipt.verification.output.source.sha256,
        receipt.completed_unix_ms,
    );
    if receipt.id != expected_id {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt id does not match its recorded execution".to_owned(),
        });
    }
    Ok(())
}

fn validate_receipt_id(receipt_id: &str) -> Result<(), AvpactError> {
    let digest = receipt_id
        .strip_prefix("rcpt_")
        .ok_or_else(|| AvpactError::ReceiptInvalid {
            message: "receipt id must start with rcpt_".to_owned(),
        })?;
    if !is_lower_hex(digest, 32) {
        return Err(AvpactError::ReceiptInvalid {
            message: "receipt id must end with 32 lowercase hexadecimal characters".to_owned(),
        });
    }
    Ok(())
}

fn is_lower_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn receipt_id(plan_id: &str, output_sha256: &str, completed_unix_ms: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plan_id.as_bytes());
    hasher.update([0]);
    hasher.update(output_sha256.as_bytes());
    hasher.update([0]);
    hasher.update(completed_unix_ms.to_le_bytes());
    format!("rcpt_{}", encode_lower(hasher.finalize()))[..37].to_owned()
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn check(
    name: impl Into<String>,
    passed: bool,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> CheckResult {
    CheckResult {
        check: name.into(),
        passed,
        expected: expected.into(),
        actual: actual.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fields_are_bounded_and_typed() {
        let fields = BTreeMap::from([
            ("frame".to_owned(), "12".to_owned()),
            ("out_time_us".to_owned(), "345000".to_owned()),
            ("total_size".to_owned(), "1024".to_owned()),
            (
                "speed".to_owned(),
                "x".repeat(MAX_PROGRESS_TEXT_CHARACTERS + 10),
            ),
            ("progress".to_owned(), "end".to_owned()),
        ]);

        let event = progress_event("plan_test", 2, &fields);

        assert_eq!(event.frame, Some(12));
        assert_eq!(event.out_time_ms, Some(345));
        assert_eq!(event.total_size_bytes, Some(1024));
        assert_eq!(
            event.speed.as_ref().map(String::len),
            Some(MAX_PROGRESS_TEXT_CHARACTERS)
        );
        assert!(matches!(event.state, ProgressState::Finished));
    }

    #[test]
    fn progress_lines_discard_content_beyond_the_bound() {
        let input = format!("speed={}\r\nprogress=end\n", "x".repeat(2_048));
        let mut reader = BufReader::with_capacity(32, input.as_bytes());
        let speed = read_bounded_line(&mut reader, 32)
            .expect("read long line")
            .expect("long line exists");
        let progress = read_bounded_line(&mut reader, 32)
            .expect("read progress line")
            .expect("progress line exists");

        assert_eq!(speed.len(), 32);
        assert_eq!(progress, "progress=end");
        assert!(
            read_bounded_line(&mut reader, 32)
                .expect("read end of input")
                .is_none()
        );
    }

    #[test]
    fn diagnostic_reader_keeps_only_the_tail() {
        let bytes = vec![b'x'; MAX_BACKEND_DIAGNOSTIC_BYTES + 50];
        let result = read_bounded_tail(bytes.as_slice());
        assert_eq!(result.len(), MAX_BACKEND_DIAGNOSTIC_BYTES);
    }

    #[test]
    fn receipt_primary_failure_uses_no_clobber_recovery_path() {
        let receipt: Receipt = serde_json::from_str(include_str!(
            "../tests/fixtures/contracts/v0.1/receipt.clip.json"
        ))
        .expect("parse receipt fixture");
        let requested = PathBuf::from("requested.json");
        let expected_recovery = recovery_receipt_path(&receipt);
        let mut attempted = Vec::new();

        let error = persist_receipt_with_recovery_using(&requested, &receipt, |path, _receipt| {
            attempted.push(path.to_path_buf());
            if path == requested {
                Err(AvpactError::ReceiptWrite {
                    path: path.to_path_buf(),
                    source: std::io::Error::other("primary failure"),
                })
            } else {
                Ok(())
            }
        })
        .expect_err("primary failure requires reconciliation");

        assert_eq!(
            attempted,
            vec![requested.clone(), expected_recovery.clone()]
        );
        assert!(matches!(
            error,
            AvpactError::ReceiptRecoveryRequired(recovery)
                if recovery.requested_receipt == requested
                    && recovery.recovery_receipt == expected_recovery
        ));
    }

    #[test]
    fn receipt_recovery_failure_retains_machine_actionable_identity() {
        let receipt: Receipt = serde_json::from_str(include_str!(
            "../tests/fixtures/contracts/v0.1/receipt.clip.json"
        ))
        .expect("parse receipt fixture");
        let requested = PathBuf::from("requested.json");
        let expected_recovery = recovery_receipt_path(&receipt);

        let error = persist_receipt_with_recovery_using(&requested, &receipt, |path, _receipt| {
            Err(AvpactError::ReceiptWrite {
                path: path.to_path_buf(),
                source: std::io::Error::other("forced failure"),
            })
        })
        .expect_err("both writes must fail");

        assert!(matches!(
            error,
            AvpactError::ReceiptRecoveryFailed(recovery)
                if recovery.output == receipt.publication.output
                    && recovery.output_sha256 == receipt.verification.output.source.sha256
                    && recovery.requested_receipt == requested
                    && recovery.recovery_receipt == expected_recovery
        ));
    }

    #[test]
    fn cancellation_token_is_shared_between_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn parses_loudnorm_integrated_loudness() {
        let diagnostic = r#"
        {
            "input_i" : "-14.37",
            "input_tp" : "-1.20"
        }
        "#;
        assert_eq!(parse_input_loudness(diagnostic), Some(-1_437));
        assert_eq!(parse_input_loudness(r#"{"input_i":"-inf"}"#), None);
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_the_backend_process_group() {
        let token = CancellationToken::new();
        let signal_token = token.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            signal_token.cancel();
        });
        let started = Instant::now();
        let result = execute_backend(
            Path::new("/bin/sh"),
            &["-c".to_owned(), "sleep 30 & wait".to_owned()],
            "plan_cancel_test",
            &token,
            None,
            &mut |_| {},
        );
        canceller.join().expect("canceller thread");

        assert!(matches!(result, Err(AvpactError::Cancelled { .. })));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn runtime_budget_terminates_the_backend_process_group() {
        let token = CancellationToken::new();
        let directory = tempfile::tempdir().expect("temporary directory");
        let temporary_path = directory.path().join("output.tmp");
        let started = Instant::now();
        let result = execute_backend(
            Path::new("/bin/sh"),
            &["-c".to_owned(), "sleep 30 & wait".to_owned()],
            "plan_budget_test",
            &token,
            Some(ExecutionLimits {
                temporary_path: &temporary_path,
                max_output_bytes: u64::MAX,
                max_temporary_bytes: u64::MAX,
                max_runtime_ms: 100,
            }),
            &mut |_| {},
        );

        assert!(matches!(
            result,
            Err(AvpactError::ResourceLimitExceeded { ref resource, .. })
                if resource == "runtime_ms"
        ));
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
