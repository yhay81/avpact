use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::error::{AvpactError, bounded_diagnostic};
use crate::hex::encode_lower;
use crate::model::{
    AudioSummary, BackendIdentity, FormatSummary, InspectionReport, ProbeDocument, ProbeStream,
    SourceIdentity, StreamKind, StreamSummary, VideoSummary,
};

const HASH_BUFFER_BYTES: usize = 64 * 1024;

pub fn inspect(path: &Path, ffprobe: &Path) -> Result<InspectionReport, AvpactError> {
    let source = identify_source(path)?;
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&source.path)
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: ffprobe.display().to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffprobe.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }

    let probe: ProbeDocument = serde_json::from_slice(&output.stdout).map_err(|source| {
        AvpactError::BackendOutputInvalid {
            backend: ffprobe.display().to_string(),
            source,
        }
    })?;
    let version = probe_version(ffprobe)?;

    Ok(normalize(source, version, probe))
}

pub fn identify_source(path: &Path) -> Result<SourceIdentity, AvpactError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(AvpactError::InputNotFound {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(AvpactError::InputRead {
                path: path.to_path_buf(),
                source,
            });
        }
    };

    if !metadata.is_file() {
        return Err(AvpactError::InputNotFile {
            path: path.to_path_buf(),
        });
    }

    let canonical_path = fs::canonicalize(path).map_err(|source| AvpactError::InputRead {
        path: path.to_path_buf(),
        source,
    })?;
    let sha256 = hash_file(&canonical_path)?;

    Ok(SourceIdentity {
        path: canonical_path,
        size_bytes: metadata.len(),
        sha256,
    })
}

fn hash_file(path: &Path) -> Result<String, AvpactError> {
    let file = File::open(path).map_err(|source| AvpactError::InputRead {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::with_capacity(HASH_BUFFER_BYTES, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_BYTES];

    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|source| AvpactError::InputRead {
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(encode_lower(hasher.finalize()))
}

pub(crate) fn probe_version(ffprobe: &Path) -> Result<String, AvpactError> {
    let output = Command::new(ffprobe)
        .arg("-version")
        .output()
        .map_err(|source| AvpactError::BackendUnavailable {
            backend: ffprobe.display().to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(AvpactError::BackendFailed {
            backend: ffprobe.display().to_string(),
            exit_code: output.status.code(),
            diagnostic: bounded_diagnostic(&output.stderr),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("ffprobe version unknown")
        .trim()
        .to_owned())
}

fn normalize(source: SourceIdentity, version: String, probe: ProbeDocument) -> InspectionReport {
    InspectionReport {
        schema_version: crate::INSPECTION_SCHEMA_VERSION.to_owned(),
        source,
        backend: BackendIdentity {
            name: "ffprobe".to_owned(),
            version,
        },
        format: FormatSummary {
            name: probe.format.format_name,
            long_name: probe.format.format_long_name,
            duration_ms: probe.format.duration.as_deref().and_then(decimal_ms),
            bit_rate_bps: probe.format.bit_rate.as_deref().and_then(parse_u64),
            tags: probe.format.tags,
        },
        streams: probe.streams.into_iter().map(normalize_stream).collect(),
    }
}

fn normalize_stream(stream: ProbeStream) -> StreamSummary {
    let kind = match stream.codec_type.as_deref() {
        Some("video") => StreamKind::Video,
        Some("audio") => StreamKind::Audio,
        Some("subtitle") => StreamKind::Subtitle,
        Some("data") => StreamKind::Data,
        Some("attachment") => StreamKind::Attachment,
        _ => StreamKind::Unknown,
    };
    let video = (kind == StreamKind::Video).then(|| VideoSummary {
        width: stream.width,
        height: stream.height,
        pixel_format: stream.pix_fmt,
        average_frame_rate: normalize_rational(stream.avg_frame_rate),
        sample_aspect_ratio: stream.sample_aspect_ratio,
        display_aspect_ratio: stream.display_aspect_ratio,
    });
    let audio = (kind == StreamKind::Audio).then(|| AudioSummary {
        sample_rate_hz: stream.sample_rate.as_deref().and_then(parse_u32),
        channels: stream.channels,
        channel_layout: stream.channel_layout,
        sample_format: stream.sample_fmt,
    });

    StreamSummary {
        index: stream.index,
        kind,
        codec: stream.codec_name,
        profile: stream.profile,
        duration_ms: stream.duration.as_deref().and_then(decimal_ms),
        bit_rate_bps: stream.bit_rate.as_deref().and_then(parse_u64),
        video,
        audio,
        disposition: stream.disposition,
        tags: stream.tags,
    }
}

fn normalize_rational(value: Option<String>) -> Option<String> {
    value.filter(|value| value != "0/0" && value != "N/A")
}

fn parse_u64(value: &str) -> Option<u64> {
    value.parse().ok()
}

fn parse_u32(value: &str) -> Option<u32> {
    value.parse().ok()
}

fn decimal_ms(value: &str) -> Option<u64> {
    if value.is_empty() || value.starts_with('-') {
        return None;
    }
    let (whole, fractional) = value.split_once('.').unwrap_or((value, ""));
    let seconds: u64 = whole.parse().ok()?;
    let mut milliseconds = 0_u64;
    for (index, byte) in fractional.bytes().take(3).enumerate() {
        if !byte.is_ascii_digit() {
            return None;
        }
        milliseconds += u64::from(byte - b'0') * 10_u64.pow(2 - index as u32);
    }
    seconds.checked_mul(1000)?.checked_add(milliseconds)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn identifies_a_file_with_a_stable_digest() {
        let mut file = NamedTempFile::new().expect("temporary file");
        file.write_all(b"avpact").expect("write fixture");

        let source = identify_source(file.path()).expect("identify fixture");

        assert_eq!(source.size_bytes, 6);
        assert_eq!(
            source.sha256,
            "7659514c499a8dd64dd3775a6c52b8dad1c9dcd9c46c8a298b59de337c7c41c6"
        );
        assert!(source.path.is_absolute());
    }

    #[test]
    fn parses_decimal_seconds_without_floating_point() {
        assert_eq!(decimal_ms("12"), Some(12_000));
        assert_eq!(decimal_ms("12.3"), Some(12_300));
        assert_eq!(decimal_ms("12.0349"), Some(12_034));
        assert_eq!(decimal_ms("N/A"), None);
        assert_eq!(decimal_ms("-0.1"), None);
    }

    #[test]
    fn normalizes_probe_output() {
        let probe: ProbeDocument =
            serde_json::from_str(include_str!("../tests/fixtures/ffprobe/sample.json"))
                .expect("valid fixture");
        let report = normalize(
            SourceIdentity {
                path: PathBuf::from("/fixture/sample.mp4"),
                size_bytes: 123,
                sha256: "abc".to_owned(),
            },
            "ffprobe version fixture".to_owned(),
            probe,
        );

        assert_eq!(report.format.duration_ms, Some(2_500));
        assert_eq!(report.format.bit_rate_bps, Some(320_000));
        assert_eq!(report.streams.len(), 2);
        assert_eq!(report.streams[0].kind, StreamKind::Video);
        assert_eq!(
            report.streams[0]
                .video
                .as_ref()
                .and_then(|video| video.average_frame_rate.as_deref()),
            Some("30000/1001")
        );
        assert_eq!(report.streams[1].kind, StreamKind::Audio);
        assert_eq!(
            report.streams[1]
                .audio
                .as_ref()
                .and_then(|audio| audio.sample_rate_hz),
            Some(48_000)
        );
    }
}
