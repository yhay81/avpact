use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InspectionReport {
    pub schema_version: String,
    pub source: SourceIdentity,
    pub backend: BackendIdentity,
    pub format: FormatSummary,
    pub streams: Vec<StreamSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BackendIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FormatSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StreamSummary {
    pub index: u32,
    pub kind: StreamKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioSummary>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub disposition: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Video,
    Audio,
    Subtitle,
    Data,
    Attachment,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_frame_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_aspect_ratio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate_hz: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProbeDocument {
    #[serde(default)]
    pub format: ProbeFormat,
    #[serde(default)]
    pub streams: Vec<ProbeStream>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ProbeFormat {
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration: Option<String>,
    pub bit_rate: Option<String>,
    pub tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProbeStream {
    pub index: u32,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub profile: Option<String>,
    pub duration: Option<String>,
    pub bit_rate: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pix_fmt: Option<String>,
    pub avg_frame_rate: Option<String>,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub sample_rate: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_fmt: Option<String>,
    #[serde(default)]
    pub disposition: BTreeMap<String, i64>,
    pub tags: Option<BTreeMap<String, String>>,
}
