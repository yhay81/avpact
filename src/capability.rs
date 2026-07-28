use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{AvpactError, bounded_diagnostic};

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CapabilityReport {
    pub schema_version: String,
    pub platform: String,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub encoders: Vec<String>,
    pub filters: Vec<String>,
    pub operations: Vec<OperationCapability>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct OperationCapability {
    pub operation: String,
    pub supported: bool,
    pub requirements: Vec<String>,
    pub missing: Vec<String>,
}

pub fn inspect_capabilities(
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<CapabilityReport, AvpactError> {
    let ffmpeg_version = first_version_line(ffmpeg)?;
    let ffprobe_version = first_version_line(ffprobe)?;
    let encoders = listing_names(ffmpeg, "-encoders")?;
    let filters = listing_names(ffmpeg, "-filters")?;
    let operations = vec![
        operation(
            "clip",
            &["encoder:libx264", "encoder:aac"],
            &encoders,
            &filters,
        ),
        operation(
            "transcode",
            &["encoder:libx264", "encoder:aac"],
            &encoders,
            &filters,
        ),
        operation(
            "resize",
            &["encoder:libx264", "encoder:aac", "filter:scale"],
            &encoders,
            &filters,
        ),
        operation("extract_audio", &["encoder:aac"], &encoders, &filters),
        operation(
            "normalize_audio",
            &["encoder:aac", "filter:loudnorm"],
            &encoders,
            &filters,
        ),
        operation(
            "concatenate",
            &["encoder:libx264", "encoder:aac", "filter:concat"],
            &encoders,
            &filters,
        ),
        operation(
            "thumbnail",
            &["encoder:mjpeg", "filter:scale"],
            &encoders,
            &filters,
        ),
        operation(
            "contact_sheet",
            &["encoder:mjpeg", "filter:fps", "filter:scale", "filter:tile"],
            &encoders,
            &filters,
        ),
        operation(
            "burn_subtitles",
            &["encoder:libx264", "encoder:aac", "filter:subtitles"],
            &encoders,
            &filters,
        ),
    ];

    Ok(CapabilityReport {
        schema_version: crate::CAPABILITY_SCHEMA_VERSION.to_owned(),
        platform: std::env::consts::OS.to_owned(),
        ffmpeg_version,
        ffprobe_version,
        encoders: encoders.into_iter().collect(),
        filters: filters.into_iter().collect(),
        operations,
    })
}

fn first_version_line(executable: &Path) -> Result<String, AvpactError> {
    let output = Command::new(executable)
        .arg("-version")
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: executable.display().to_string(),
            source,
        })?;
    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: executable.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("version unknown")
        .trim()
        .to_owned())
}

fn listing_names(ffmpeg: &Path, listing_argument: &str) -> Result<BTreeSet<String>, AvpactError> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", listing_argument])
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
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned)
        .collect())
}

fn operation(
    name: &str,
    requirements: &[&str],
    encoders: &BTreeSet<String>,
    filters: &BTreeSet<String>,
) -> OperationCapability {
    let missing: Vec<String> = requirements
        .iter()
        .filter(|requirement| {
            let (kind, capability) = requirement
                .split_once(':')
                .expect("static requirement is typed");
            match kind {
                "encoder" => !encoders.contains(capability),
                "filter" => !filters.contains(capability),
                _ => true,
            }
        })
        .map(|requirement| (*requirement).to_owned())
        .collect();
    OperationCapability {
        operation: name.to_owned(),
        supported: missing.is_empty(),
        requirements: requirements
            .iter()
            .map(|requirement| (*requirement).to_owned())
            .collect(),
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_reports_missing_capabilities() {
        let encoders = BTreeSet::from(["aac".to_owned()]);
        let filters = BTreeSet::new();
        let capability = operation(
            "normalize_audio",
            &["encoder:aac", "filter:loudnorm"],
            &encoders,
            &filters,
        );
        assert!(!capability.supported);
        assert_eq!(capability.missing, ["filter:loudnorm"]);
    }
}
