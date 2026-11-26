use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SdkError {
    InputTooLarge(usize),
    InvalidInputUtf8(String),
    Json(String),
    HostError(String),
    UnexpectedResponse(String),
    MockNotInitialized,
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge(size) => write!(f, "IPC payload too large: {size} bytes"),
            Self::InvalidInputUtf8(msg) => write!(f, "invalid UTF-8 input: {msg}"),
            Self::Json(msg) => write!(f, "JSON error: {msg}"),
            Self::HostError(msg) => write!(f, "host error: {msg}"),
            Self::UnexpectedResponse(msg) => write!(f, "unexpected host response: {msg}"),
            Self::MockNotInitialized => write!(f, "mock host is not initialized on this thread"),
        }
    }
}

impl std::error::Error for SdkError {}

fn default_envelope_version() -> u32 {
    1
}

fn default_envelope_kind() -> String {
    "hook".to_owned()
}

fn default_principal() -> String {
    "system".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    #[serde(default = "default_envelope_version")]
    pub envelope_version: u32,
    #[serde(default = "default_envelope_kind")]
    pub kind: String,
    pub event: String,
    #[serde(default)]
    pub occurred_at_ms: u64,
    #[serde(default)]
    pub correlation_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default = "default_principal")]
    pub principal: String,
    #[serde(default)]
    pub dispatch_depth: u8,
    #[serde(default)]
    pub payload: Value,
}

/// Supports both legacy v1 top-level fields (`found`/`value`/`status`/`headers`/`body`)
/// and Spec 2 responses (`data: Option<Value>`).
#[derive(Debug, Clone, Deserialize)]
pub struct HostResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub found: Option<bool>,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub status: Option<u16>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KvItem {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpRequestInput<'a> {
    pub method: &'a str,
    pub url: &'a str,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentSaveInput {
    pub type_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentEntry {
    pub id: i64,
    pub type_key: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub values: serde_json::Map<String, Value>,
    #[serde(default)]
    pub term_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentListResult {
    #[serde(default)]
    pub items: Vec<ContentEntry>,
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashEntrySummary {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub publication_status: String,
    #[serde(default)]
    pub version: i64,
    pub trashed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentTrashListResult {
    #[serde(default)]
    pub items: Vec<TrashEntrySummary>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyTerm {
    pub id: i64,
    pub kind: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub url: String,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub alt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum BeforeSaveDecision {
    Allow,
    Veto {
        code: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        field_errors: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPrepareSavePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values_patch: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ContactBeforeSubmitDecision {
    Allow,
    Reject {
        code: String,
        #[serde(default)]
        message: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeoHeadOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub og_image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noindex: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub json_ld: Vec<Value>,
}

pub type EntryViewOutput = serde_json::Map<String, Value>;
