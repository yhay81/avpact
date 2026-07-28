use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

const MAX_DIAGNOSTIC_BYTES: usize = 8 * 1024;

#[derive(Debug, Error)]
pub enum AvpactError {
    #[error("input does not exist: {path}")]
    InputNotFound { path: PathBuf },

    #[error("input is not a regular file: {path}")]
    InputNotFile { path: PathBuf },

    #[error("cannot read input {path}: {source}")]
    InputRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot start {backend}: {source}")]
    BackendUnavailable {
        backend: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{backend} failed with exit code {exit_code:?}")]
    BackendFailed {
        backend: String,
        exit_code: Option<i32>,
        diagnostic: String,
    },

    #[error("{backend} returned malformed JSON: {source}")]
    BackendOutputInvalid {
        backend: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("cannot serialize output: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("cannot read recipe {path}: {source}")]
    RecipeRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("recipe is invalid: {message}")]
    RecipeInvalid { message: String },

    #[error("request is unsupported: {message}")]
    Unsupported { message: String },

    #[error("output already exists and overwrite policy is deny: {path}")]
    OutputExists { path: PathBuf },

    #[error("output path is invalid: {path}: {message}")]
    OutputPathInvalid { path: PathBuf, message: String },

    #[error("input and output resolve to the same file: {path}")]
    InputOutputConflict { path: PathBuf },

    #[error("cannot write plan {path}: {source}")]
    PlanWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot read plan {path}: {source}")]
    PlanRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("plan is invalid: {message}")]
    PlanInvalid { message: String },

    #[error("input changed after planning: {path}")]
    InputChanged { path: PathBuf },

    #[error("temporary output already exists: {path}")]
    TemporaryOutputExists { path: PathBuf },

    #[error("cannot execute backend {backend}: {source}")]
    BackendExecution {
        backend: String,
        #[source]
        source: std::io::Error,
    },

    #[error("backend build identity changed after planning; planned {planned}, found {actual}")]
    BackendIdentityMismatch { planned: String, actual: String },

    #[error("operation cancelled for plan {plan_id}")]
    Cancelled { plan_id: String },

    #[error("cannot install cancellation handler: {message}")]
    CancellationSetup { message: String },

    #[error("verification failed for temporary output {path}: {summary}")]
    VerificationFailed { path: PathBuf, summary: String },

    #[error("verification measurement failed: {message}")]
    VerificationMeasurement { message: String },

    #[error("resource limit exceeded for {resource}: limit {limit}, observed {actual}")]
    ResourceLimitExceeded {
        resource: String,
        limit: u64,
        actual: u64,
    },

    #[error("cannot inspect resource availability for {path}: {source}")]
    ResourceCheck {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("cannot publish verified output {path}: {source}")]
    PublishFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("receipt already exists: {path}")]
    ReceiptExists { path: PathBuf },

    #[error("cannot write receipt {path}: {source}")]
    ReceiptWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "receipt persistence failed and published output {output} could not be safely rolled back: {message}"
    )]
    ReceiptRollbackFailed { output: PathBuf, message: String },

    #[error("cannot read receipt {path}: {source}")]
    ReceiptRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("receipt is invalid: {message}")]
    ReceiptInvalid { message: String },
}

impl AvpactError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InputNotFound { .. } => "input_not_found",
            Self::InputNotFile { .. } => "input_not_file",
            Self::InputRead { .. } => "input_read_failed",
            Self::BackendUnavailable { .. } => "backend_unavailable",
            Self::BackendFailed { .. } => "backend_failed",
            Self::BackendOutputInvalid { .. } => "backend_output_invalid",
            Self::Serialization(_) => "serialization_failed",
            Self::RecipeRead { .. } => "recipe_read_failed",
            Self::RecipeInvalid { .. } => "recipe_invalid",
            Self::Unsupported { .. } => "unsupported",
            Self::OutputExists { .. } => "output_exists",
            Self::OutputPathInvalid { .. } => "output_path_invalid",
            Self::InputOutputConflict { .. } => "input_output_conflict",
            Self::PlanWrite { .. } => "plan_write_failed",
            Self::PlanRead { .. } => "plan_read_failed",
            Self::PlanInvalid { .. } => "plan_invalid",
            Self::InputChanged { .. } => "input_changed",
            Self::TemporaryOutputExists { .. } => "temporary_output_exists",
            Self::BackendExecution { .. } => "backend_execution_failed",
            Self::BackendIdentityMismatch { .. } => "backend_identity_mismatch",
            Self::Cancelled { .. } => "cancelled",
            Self::CancellationSetup { .. } => "cancellation_setup_failed",
            Self::VerificationFailed { .. } => "verification_failed",
            Self::VerificationMeasurement { .. } => "verification_measurement_failed",
            Self::ResourceLimitExceeded { .. } => "resource_limit_exceeded",
            Self::ResourceCheck { .. } => "resource_check_failed",
            Self::PublishFailed { .. } => "publish_failed",
            Self::ReceiptExists { .. } => "receipt_exists",
            Self::ReceiptWrite { .. } => "receipt_write_failed",
            Self::ReceiptRollbackFailed { .. } => "receipt_rollback_failed",
            Self::ReceiptRead { .. } => "receipt_read_failed",
            Self::ReceiptInvalid { .. } => "receipt_invalid",
        }
    }

    pub fn diagnostic(&self) -> Option<&str> {
        match self {
            Self::BackendFailed { diagnostic, .. } if !diagnostic.is_empty() => {
                Some(diagnostic.as_str())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorDocument<'a> {
    pub schema_version: &'static str,
    pub error: ErrorBody<'a>,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody<'a> {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<&'a str>,
}

impl<'a> From<&'a AvpactError> for ErrorDocument<'a> {
    fn from(error: &'a AvpactError) -> Self {
        Self {
            schema_version: crate::ERROR_SCHEMA_VERSION,
            error: ErrorBody {
                code: error.code(),
                message: error.to_string(),
                diagnostic: error.diagnostic(),
            },
        }
    }
}

pub(crate) fn bounded_diagnostic(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(MAX_DIAGNOSTIC_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_keep_only_the_bounded_tail() {
        let input = vec![b'x'; MAX_DIAGNOSTIC_BYTES + 100];
        let result = bounded_diagnostic(&input);
        assert_eq!(result.len(), MAX_DIAGNOSTIC_BYTES);
    }
}
