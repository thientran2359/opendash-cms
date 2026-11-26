use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    id::{valid_namespaced_block_id, valid_snake_key_with_limit},
    richtext::{MAX_RICHTEXT_OUTPUT_BYTES, sanitize_richtext, strip_richtext_to_plain},
};

pub const MAX_TOP_REAL_FIELDS: usize = 32;
pub const MAX_TOTAL_LEAF_FIELDS: usize = 48;
pub const MAX_REPEATER_ITEMS: usize = 32;
pub const MAX_TOTAL_REPEATER_ITEMS: usize = 64;
pub const MAX_NODE_VALUES_KEYS: usize = 64;
pub const MAX_NODE_VALUES_BYTES: usize = 65_536;
pub const MAX_BLOCK_URL_BYTES: usize = 2048;

pub const ALLOWED_COLOR_TOKENS: [&str; 6] = [
    "var(--od-token-primary)",
    "var(--od-token-accent)",
    "var(--od-token-surface)",
    "var(--od-token-muted)",
    "var(--od-token-text)",
    "var(--od-token-border)",
];

pub const GLOBAL_PUBLIC_URL_OPTION_KEYS: &[&str] = &[
    "social_facebook",
    "social_youtube",
    "social_tiktok",
    "social_instagram",
    "social_x",
    "social_linkedin",
    "social_email",
];
pub const GLOBAL_PUBLIC_MEDIA_OPTION_KEYS: &[&str] = &["site_logo"];

pub fn global_site_option_fields() -> Vec<BlockFieldSpec> {
    vec![
        BlockFieldSpec::Group {
            key: "social_links_group".to_owned(),
            label: "Mạng xã hội".to_owned(),
            show_if: None,
            fields: vec![
                BlockFieldSpec::Url {
                    key: "social_facebook".to_owned(),
                    label: "Facebook".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://facebook.com/yourpage".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_youtube".to_owned(),
                    label: "YouTube".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://youtube.com/@yourchannel".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_tiktok".to_owned(),
                    label: "TikTok".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://tiktok.com/@yourhandle".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_instagram".to_owned(),
                    label: "Instagram".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://instagram.com/yourhandle".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_x".to_owned(),
                    label: "X (Twitter)".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://x.com/yourhandle".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_linkedin".to_owned(),
                    label: "LinkedIn".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("https://linkedin.com/in/yourprofile".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Url {
                    key: "social_email".to_owned(),
                    label: "Email liên hệ".to_owned(),
                    required: false,
                    default: None,
                    placeholder: Some("mailto:hello@example.com".to_owned()),
                    show_if: None,
                },
            ],
        },
        BlockFieldSpec::Group {
            key: "brand_contact_group".to_owned(),
            label: "Thương hiệu & Liên hệ chung".to_owned(),
            show_if: None,
            fields: vec![
                BlockFieldSpec::Media {
                    key: "site_logo".to_owned(),
                    label: "Logo website".to_owned(),
                    media_kind: MediaFieldKind::Image,
                    required: false,
                    default: None,
                    css_var: None,
                    show_if: None,
                },
                BlockFieldSpec::Color {
                    key: "brand_primary_color".to_owned(),
                    label: "Màu chủ đạo".to_owned(),
                    required: false,
                    default: Some("#2563eb".to_owned()),
                    css_var: None,
                    show_if: None,
                },
                BlockFieldSpec::Text {
                    key: "contact_phone".to_owned(),
                    label: "Số điện thoại".to_owned(),
                    max_chars: Some(40),
                    required: false,
                    default: None,
                    placeholder: Some("0901 234 567".to_owned()),
                    show_if: None,
                },
                BlockFieldSpec::Textarea {
                    key: "contact_address".to_owned(),
                    label: "Địa chỉ".to_owned(),
                    max_chars: Some(300),
                    required: false,
                    default: None,
                    show_if: None,
                },
                BlockFieldSpec::Text {
                    key: "footer_note".to_owned(),
                    label: "Ghi chú chân trang".to_owned(),
                    max_chars: Some(240),
                    required: false,
                    default: None,
                    placeholder: Some("© 2026 OpenDash".to_owned()),
                    show_if: None,
                },
            ],
        },
    ]
}

pub fn default_value_for_field_spec(spec: &BlockFieldSpec) -> Value {
    if let Some(v) = spec.default_json_value() {
        return v;
    }
    match spec {
        BlockFieldSpec::Text { .. }
        | BlockFieldSpec::Textarea { .. }
        | BlockFieldSpec::Richtext { .. }
        | BlockFieldSpec::Color { .. }
        | BlockFieldSpec::Media { .. }
        | BlockFieldSpec::Url { .. } => Value::String(String::new()),
        BlockFieldSpec::Number { .. } => serde_json::json!(0),
        BlockFieldSpec::Range { min, .. } => serde_json::json!(min),
        BlockFieldSpec::Boolean { .. } => Value::Bool(false),
        BlockFieldSpec::Select { options, .. } | BlockFieldSpec::RadioButtons { options, .. } => {
            options
                .first()
                .map(|o| Value::String(o.value.clone()))
                .unwrap_or_else(|| Value::String(String::new()))
        }
        BlockFieldSpec::Spacing { .. } => serde_json::json!({}),
        BlockFieldSpec::Link { .. } => {
            serde_json::json!({ "url": "", "new_tab": false, "nofollow": false })
        }
        BlockFieldSpec::Repeater { .. } => serde_json::json!([]),
        BlockFieldSpec::Group { .. } => Value::Null,
    }
}

pub fn global_site_option_defaults() -> BTreeMap<String, Value> {
    let fields = global_site_option_fields();
    let mut out = BTreeMap::new();
    for leaf in flattened_field_specs(&fields) {
        out.insert(leaf.key().to_owned(), default_value_for_field_spec(leaf));
    }
    out
}

const fn default_repeater_max() -> usize {
    32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockCategory {
    Layout,
    Media,
    Interactive,
    Content,
    Marketing,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaFieldKind {
    Image,
    Video,
    #[default]
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CssUnit {
    Px,
    Rem,
    Em,
    #[serde(rename = "%")]
    Percent,
    Vw,
    Vh,
}

impl CssUnit {
    pub fn as_css_suffix(self) -> &'static str {
        match self {
            CssUnit::Px => "px",
            CssUnit::Rem => "rem",
            CssUnit::Em => "em",
            CssUnit::Percent => "%",
            CssUnit::Vw => "vw",
            CssUnit::Vh => "vh",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectDisplay {
    Dropdown,
    Buttons,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpacingValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<f64>,
}

impl SpacingValue {
    pub fn sides(&self) -> [Option<f64>; 4] {
        [self.top, self.right, self.bottom, self.left]
    }

    pub fn format_css(&self, unit: CssUnit) -> Option<String> {
        let sides = self.sides();
        if sides.iter().all(Option::is_none) {
            return None;
        }
        let suffix = unit.as_css_suffix();
        let mut parts = Vec::with_capacity(4);
        for side in sides {
            let n = side.unwrap_or(0.0);
            if !n.is_finite() {
                return None;
            }
            if n.fract() == 0.0 && n.abs() < 1e15 {
                parts.push(format!("{}{suffix}", n as i64));
            } else {
                parts.push(format!("{n}{suffix}"));
            }
        }
        Some(parts.join(" "))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkValue {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub new_tab: bool,
    #[serde(default)]
    pub nofollow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShowIfScalar {
    Bool(bool),
    Int(i64),
    String(String),
    Null,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShowIfRule {
    pub field: String,
    pub equals: ShowIfScalar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockFieldSpec {
    Text {
        key: String,
        label: String,
        #[serde(default)]
        max_chars: Option<usize>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Textarea {
        key: String,
        label: String,
        #[serde(default)]
        max_chars: Option<usize>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Richtext {
        key: String,
        label: String,
        #[serde(default)]
        max_bytes: Option<usize>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Number {
        key: String,
        label: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        unit: Option<CssUnit>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        responsive: bool,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Range {
        key: String,
        label: String,
        min: f64,
        max: f64,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        unit: Option<CssUnit>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        responsive: bool,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Boolean {
        key: String,
        label: String,
        #[serde(default)]
        default: Option<bool>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Select {
        key: String,
        label: String,
        options: Vec<SelectOption>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<SelectDisplay>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        responsive: bool,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    RadioButtons {
        key: String,
        label: String,
        options: Vec<SelectOption>,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        responsive: bool,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Spacing {
        key: String,
        label: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        unit: Option<CssUnit>,
        #[serde(default)]
        default: Option<SpacingValue>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        responsive: bool,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Link {
        key: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<LinkValue>,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Color {
        key: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Media {
        key: String,
        label: String,
        #[serde(default)]
        media_kind: MediaFieldKind,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        css_var: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Url {
        key: String,
        label: String,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        placeholder: Option<String>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Group {
        key: String,
        label: String,
        fields: Vec<BlockFieldSpec>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
    Repeater {
        key: String,
        label: String,
        #[serde(default)]
        min_items: usize,
        #[serde(default = "default_repeater_max")]
        max_items: usize,
        #[serde(default)]
        item_label_field: Option<String>,
        fields: Vec<BlockFieldSpec>,
        #[serde(default)]
        show_if: Option<ShowIfRule>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSchemaV1 {
    pub schema_version: u8,
    pub id: String,
    pub title: String,
    pub category: BlockCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub render: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_parents: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_children: Option<Vec<String>>,
    #[serde(default)]
    pub fields: Vec<BlockFieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredBlockSchema {
    pub schema: BlockSchemaV1,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectedBlockMediaIds {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub any_ids: BTreeSet<String>,
}

impl BlockFieldSpec {
    pub fn key(&self) -> &str {
        match self {
            BlockFieldSpec::Text { key, .. }
            | BlockFieldSpec::Textarea { key, .. }
            | BlockFieldSpec::Richtext { key, .. }
            | BlockFieldSpec::Number { key, .. }
            | BlockFieldSpec::Range { key, .. }
            | BlockFieldSpec::Boolean { key, .. }
            | BlockFieldSpec::Select { key, .. }
            | BlockFieldSpec::RadioButtons { key, .. }
            | BlockFieldSpec::Spacing { key, .. }
            | BlockFieldSpec::Color { key, .. }
            | BlockFieldSpec::Media { key, .. }
            | BlockFieldSpec::Url { key, .. }
            | BlockFieldSpec::Link { key, .. }
            | BlockFieldSpec::Group { key, .. }
            | BlockFieldSpec::Repeater { key, .. } => key,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            BlockFieldSpec::Text { label, .. }
            | BlockFieldSpec::Textarea { label, .. }
            | BlockFieldSpec::Richtext { label, .. }
            | BlockFieldSpec::Number { label, .. }
            | BlockFieldSpec::Range { label, .. }
            | BlockFieldSpec::Boolean { label, .. }
            | BlockFieldSpec::Select { label, .. }
            | BlockFieldSpec::RadioButtons { label, .. }
            | BlockFieldSpec::Spacing { label, .. }
            | BlockFieldSpec::Color { label, .. }
            | BlockFieldSpec::Media { label, .. }
            | BlockFieldSpec::Url { label, .. }
            | BlockFieldSpec::Link { label, .. }
            | BlockFieldSpec::Group { label, .. }
            | BlockFieldSpec::Repeater { label, .. } => label,
        }
    }

    pub fn show_if(&self) -> Option<&ShowIfRule> {
        match self {
            BlockFieldSpec::Text { show_if, .. }
            | BlockFieldSpec::Textarea { show_if, .. }
            | BlockFieldSpec::Richtext { show_if, .. }
            | BlockFieldSpec::Number { show_if, .. }
            | BlockFieldSpec::Range { show_if, .. }
            | BlockFieldSpec::Boolean { show_if, .. }
            | BlockFieldSpec::Select { show_if, .. }
            | BlockFieldSpec::RadioButtons { show_if, .. }
            | BlockFieldSpec::Spacing { show_if, .. }
            | BlockFieldSpec::Color { show_if, .. }
            | BlockFieldSpec::Media { show_if, .. }
            | BlockFieldSpec::Url { show_if, .. }
            | BlockFieldSpec::Link { show_if, .. }
            | BlockFieldSpec::Group { show_if, .. }
            | BlockFieldSpec::Repeater { show_if, .. } => show_if.as_ref(),
        }
    }

    pub fn is_responsive(&self) -> bool {
        matches!(
            self,
            BlockFieldSpec::Number {
                responsive: true,
                ..
            } | BlockFieldSpec::Range {
                responsive: true,
                ..
            } | BlockFieldSpec::Select {
                responsive: true,
                ..
            } | BlockFieldSpec::RadioButtons {
                responsive: true,
                ..
            } | BlockFieldSpec::Spacing {
                responsive: true,
                ..
            }
        )
    }

    pub fn default_json_value(&self) -> Option<Value> {
        match self {
            BlockFieldSpec::Text { default, .. }
            | BlockFieldSpec::Textarea { default, .. }
            | BlockFieldSpec::Richtext { default, .. }
            | BlockFieldSpec::Select { default, .. }
            | BlockFieldSpec::RadioButtons { default, .. }
            | BlockFieldSpec::Color { default, .. }
            | BlockFieldSpec::Media { default, .. }
            | BlockFieldSpec::Url { default, .. } => {
                default.as_ref().map(|s| Value::String(s.clone()))
            }
            BlockFieldSpec::Number { default, .. } | BlockFieldSpec::Range { default, .. } => {
                default.and_then(|n| serde_json::Number::from_f64(n).map(Value::Number))
            }
            BlockFieldSpec::Boolean { default, .. } => default.map(Value::Bool),
            BlockFieldSpec::Spacing { default, .. } => {
                default.and_then(|s| serde_json::to_value(s).ok())
            }
            BlockFieldSpec::Link { default, .. } => {
                default.as_ref().and_then(|l| serde_json::to_value(l).ok())
            }
            BlockFieldSpec::Group { .. } | BlockFieldSpec::Repeater { .. } => None,
        }
    }
}

pub fn valid_block_url(raw: &str) -> bool {
    if raw.is_empty() || raw.len() > MAX_BLOCK_URL_BYTES {
        return false;
    }
    if raw
        .bytes()
        .any(|b| b <= 0x1F || b == 0x7F || b.is_ascii_whitespace())
        || raw.chars().any(char::is_whitespace)
    {
        return false;
    }
    if raw == "#" {
        return true;
    }
    if let Some(fragment) = raw.strip_prefix('#') {
        return !fragment.is_empty()
            && fragment.len() <= 128
            && fragment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    }
    if raw.starts_with('/') {
        return !raw.starts_with("//") && !raw.contains('\\') && !raw.contains("..");
    }
    let lower_prefix_8 = raw.get(..8).map(str::to_ascii_lowercase);
    let lower_prefix_7 = raw.get(..7).map(str::to_ascii_lowercase);
    let lower_prefix_4 = raw.get(..4).map(str::to_ascii_lowercase);

    if lower_prefix_7.as_deref() == Some("mailto:") {
        let addr = &raw[7..];
        let email_part = addr.split('?').next().unwrap_or("");
        let Some((local, domain)) = email_part.split_once('@') else {
            return false;
        };
        return !local.is_empty()
            && !domain.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !addr.contains('\\');
    }
    if lower_prefix_4.as_deref() == Some("tel:") {
        let number = &raw[4..];
        return !number.is_empty()
            && number.len() <= 64
            && number.bytes().all(|b| {
                b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'(' | b')' | b'*' | b'#')
            })
            && number.bytes().any(|b| b.is_ascii_digit());
    }
    if lower_prefix_7.as_deref() == Some("http://") || lower_prefix_8.as_deref() == Some("https://")
    {
        let Ok(parsed) = Url::parse(raw) else {
            return false;
        };
        return matches!(parsed.scheme(), "http" | "https")
            && parsed.has_host()
            && parsed.username().is_empty()
            && parsed.password().is_none();
    }
    false
}

pub fn valid_block_color(raw: &str) -> bool {
    if raw == "transparent" || raw == "currentColor" || ALLOWED_COLOR_TOKENS.contains(&raw) {
        return true;
    }
    let Some(hex) = raw.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn valid_css_var_name(raw: &str) -> bool {
    let Some(suffix) = raw.strip_prefix("--od-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 48
        && suffix
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn valid_css_select_value(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub fn valid_sha256_media_id(raw: &str) -> bool {
    raw.len() == 64 && raw.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_icon_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check_scope_reserved_key(
    spec: &BlockFieldSpec,
    scope_media_keys: &BTreeSet<&str>,
    scope_link_keys: &BTreeSet<&str>,
    scope_spacing_keys: &BTreeSet<&str>,
) -> Result<(), &'static str> {
    let key = spec.key();
    if !valid_snake_key_with_limit(key, 32) {
        return Err("Khóa field block không hợp lệ (snake_case tối đa 32 ký tự).");
    }
    for suffix in ["_url", "_alt", "_width", "_height"] {
        if let Some(prefix) = key.strip_suffix(suffix)
            && scope_media_keys.contains(prefix)
        {
            return Err(
                "Khóa field không được trùng biến tổng hợp <media_key>_{url,alt,width,height} của field Media cùng phạm vi.",
            );
        }
    }
    for suffix in ["_url", "_target", "_rel", "_new_tab", "_nofollow"] {
        if let Some(prefix) = key.strip_suffix(suffix)
            && scope_link_keys.contains(prefix)
        {
            return Err(
                "Khóa field không được trùng biến tổng hợp <link_key>_{url,target,rel,new_tab,nofollow} của field Link cùng phạm vi.",
            );
        }
    }
    if let Some(prefix) = key.strip_suffix("_classes")
        && scope_spacing_keys.contains(prefix)
    {
        return Err(
            "Khóa field không được trùng biến tổng hợp <spacing_key>_classes của field Spacing cùng phạm vi.",
        );
    }
    if !matches!(
        spec,
        BlockFieldSpec::Url { .. } | BlockFieldSpec::Link { .. }
    ) && (key == "url" || key.ends_with("_url"))
    {
        return Err("Chỉ field kiểu Url hoặc Link mới được dùng tên 'url' hoặc hậu tố '_url'.");
    }
    Ok(())
}

fn validate_label(label: &str) -> Result<(), &'static str> {
    if label.trim().is_empty() || label.chars().count() > 80 {
        return Err("Nhãn field block phải không rỗng và tối đa 80 ký tự.");
    }
    Ok(())
}

fn collect_css_var<'a>(
    css_var: Option<&'a str>,
    seen_css_vars: &mut BTreeSet<&'a str>,
) -> Result<(), &'static str> {
    if let Some(var) = css_var {
        if !valid_css_var_name(var) {
            return Err("Tên biến CSS phải khớp ^--od-[a-z0-9-]{1,48}$.");
        }
        if !seen_css_vars.insert(var) {
            return Err("Biến CSS không được trùng lặp trong cùng một block.");
        }
    }
    Ok(())
}

fn validate_primitive_field<'a>(
    spec: &'a BlockFieldSpec,
    in_repeater: bool,
    seen_css_vars: &mut BTreeSet<&'a str>,
) -> Result<(), &'static str> {
    validate_label(spec.label())?;
    match spec {
        BlockFieldSpec::Text {
            max_chars,
            default,
            placeholder,
            ..
        } => {
            let limit = max_chars.unwrap_or(256);
            if limit == 0 || limit > 2000 {
                return Err("max_chars của field Text phải nằm trong 1..=2000.");
            }
            if let Some(def) = default
                && def.chars().count() > limit
            {
                return Err("Giá trị mặc định Text vượt quá max_chars.");
            }
            if let Some(ph) = placeholder
                && ph.chars().count() > 120
            {
                return Err("Placeholder vượt quá 120 ký tự.");
            }
        }
        BlockFieldSpec::Textarea {
            max_chars, default, ..
        } => {
            let limit = max_chars.unwrap_or(4000);
            if limit == 0 || limit > 16_384 {
                return Err("max_chars của field Textarea phải nằm trong 1..=16384.");
            }
            if let Some(def) = default
                && def.chars().count() > limit
            {
                return Err("Giá trị mặc định Textarea vượt quá max_chars.");
            }
        }
        BlockFieldSpec::Richtext {
            max_bytes, default, ..
        } => {
            let limit = max_bytes.unwrap_or(MAX_RICHTEXT_OUTPUT_BYTES);
            if limit == 0 || limit > MAX_RICHTEXT_OUTPUT_BYTES {
                return Err("max_bytes của field Richtext phải nằm trong 1..=32768.");
            }
            if let Some(def) = default {
                let cleaned = sanitize_richtext(def)?;
                if cleaned.len() > limit {
                    return Err("Giá trị mặc định Richtext vượt quá max_bytes sau làm sạch.");
                }
            }
        }
        BlockFieldSpec::Number {
            min,
            max,
            step,
            default,
            css_var,
            responsive,
            ..
        } => {
            if in_repeater && (*responsive || css_var.is_some()) {
                return Err("Field bên trong Repeater không hỗ trợ responsive hoặc css_var.");
            }
            if let Some(mn) = min
                && !mn.is_finite()
            {
                return Err("Giá trị min của Number phải hữu hạn.");
            }
            if let Some(mx) = max
                && !mx.is_finite()
            {
                return Err("Giá trị max của Number phải hữu hạn.");
            }
            if let (Some(mn), Some(mx)) = (min, max)
                && mn > mx
            {
                return Err("Number có min lớn hơn max.");
            }
            if let Some(st) = step
                && (!st.is_finite() || *st <= 0.0)
            {
                return Err("step của Number phải hữu hạn và > 0.");
            }
            if let Some(def) = default {
                if !def.is_finite() {
                    return Err("Giá trị mặc định Number phải hữu hạn.");
                }
                if min.is_some_and(|mn| *def < mn) || max.is_some_and(|mx| *def > mx) {
                    return Err("Giá trị mặc định Number nằm ngoài [min, max].");
                }
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Range {
            min,
            max,
            step,
            default,
            css_var,
            responsive,
            ..
        } => {
            if in_repeater && (*responsive || css_var.is_some()) {
                return Err("Field bên trong Repeater không hỗ trợ responsive hoặc css_var.");
            }
            if !min.is_finite() || !max.is_finite() || min > max {
                return Err("Khoảng min/max của Range không hợp lệ.");
            }
            if let Some(st) = step
                && (!st.is_finite() || *st <= 0.0)
            {
                return Err("step của Range phải hữu hạn và > 0.");
            }
            if let Some(def) = default
                && (!def.is_finite() || *def < *min || *def > *max)
            {
                return Err("Giá trị mặc định Range nằm ngoài [min, max].");
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Boolean { .. } => {}
        BlockFieldSpec::Select {
            options,
            default,
            css_var,
            responsive,
            ..
        }
        | BlockFieldSpec::RadioButtons {
            options,
            default,
            css_var,
            responsive,
            ..
        } => {
            if in_repeater && (*responsive || css_var.is_some()) {
                return Err("Field bên trong Repeater không hỗ trợ responsive hoặc css_var.");
            }
            let max_opts = if matches!(spec, BlockFieldSpec::RadioButtons { .. }) {
                8
            } else {
                32
            };
            if options.is_empty() || options.len() > max_opts {
                return Err("Số lượng lựa chọn của Select/RadioButtons vượt giới hạn hợp lệ.");
            }
            let mut seen_opts = BTreeSet::new();
            for opt in options {
                if opt.value.is_empty() || opt.value.chars().count() > 64 {
                    return Err("Giá trị option Select phải không rỗng và <= 64 ký tự.");
                }
                if !seen_opts.insert(opt.value.as_str()) {
                    return Err("Giá trị option Select không được trùng lặp.");
                }
                validate_label(&opt.label)?;
                if let Some(icon) = &opt.icon
                    && !valid_icon_name(icon)
                {
                    return Err("Tên icon option chỉ được gồm [a-z0-9-] (1..=32 ký tự).");
                }
                if css_var.is_some() && !valid_css_select_value(&opt.value) {
                    return Err("Option Select có css_var phải khớp ^[a-z0-9_-]{1,32}$.");
                }
            }
            if let Some(def) = default
                && !seen_opts.contains(def.as_str())
            {
                return Err("Giá trị mặc định Select không có trong danh sách options.");
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Spacing {
            min,
            max,
            default,
            css_var,
            responsive,
            ..
        } => {
            if in_repeater && (*responsive || css_var.is_some()) {
                return Err("Field bên trong Repeater không hỗ trợ responsive hoặc css_var.");
            }
            let lo = min.unwrap_or(0.0);
            let hi = max.unwrap_or(256.0);
            if !lo.is_finite() || !hi.is_finite() || lo > hi {
                return Err("Khoảng min/max của Spacing không hợp lệ.");
            }
            if let Some(def) = default {
                for side in def.sides().into_iter().flatten() {
                    if !side.is_finite() || side < lo || side > hi {
                        return Err("Giá trị mặc định Spacing nằm ngoài [min, max].");
                    }
                }
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Color {
            default, css_var, ..
        } => {
            if in_repeater && css_var.is_some() {
                return Err("Field bên trong Repeater không hỗ trợ css_var.");
            }
            if let Some(def) = default
                && !valid_block_color(def)
            {
                return Err("Màu mặc định không hợp lệ.");
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Media {
            media_kind,
            default,
            css_var,
            ..
        } => {
            if in_repeater && css_var.is_some() {
                return Err("Field bên trong Repeater không hỗ trợ css_var.");
            }
            if css_var.is_some() && *media_kind != MediaFieldKind::Image {
                return Err("Field Media gắn css_var bắt buộc có media_kind: \"image\".");
            }
            if let Some(def) = default
                && !def.is_empty()
                && !valid_sha256_media_id(def)
            {
                return Err("Media ID mặc định phải là chuỗi SHA-256 64 ký tự hex.");
            }
            collect_css_var(css_var.as_deref(), seen_css_vars)?;
        }
        BlockFieldSpec::Url {
            default,
            placeholder,
            ..
        } => {
            if let Some(def) = default
                && !def.is_empty()
                && !valid_block_url(def)
            {
                return Err("URL mặc định của block không hợp lệ.");
            }
            if let Some(ph) = placeholder
                && ph.chars().count() > 120
            {
                return Err("Placeholder Url vượt quá 120 ký tự.");
            }
        }
        BlockFieldSpec::Link {
            default,
            placeholder,
            ..
        } => {
            if let Some(def) = default
                && !def.url.is_empty()
                && !valid_block_url(&def.url)
            {
                return Err("URL mặc định trong field Link không hợp lệ.");
            }
            if let Some(ph) = placeholder
                && ph.chars().count() > 120
            {
                return Err("Placeholder Link vượt quá 120 ký tự.");
            }
        }
        BlockFieldSpec::Group { .. } | BlockFieldSpec::Repeater { .. } => {
            return Err("Không thể lồng Group/Repeater tại đây.");
        }
    }
    Ok(())
}

impl BlockSchemaV1 {
    pub fn validate(&self, expected_owner_prefix: &str) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("block.json chỉ hỗ trợ schema_version == 1.");
        }
        if self.render != "render.html" {
            return Err("block.json bắt buộc khai báo render == \"render.html\".");
        }
        if self.title.trim().is_empty() || self.title.chars().count() > 80 {
            return Err("Tiêu đề block phải không rỗng và <= 80 ký tự.");
        }
        if let Some(icon) = &self.icon
            && !valid_icon_name(icon)
        {
            return Err("Tên icon block chỉ được gồm [a-z0-9-] (1..=32 ký tự).");
        }
        if !valid_namespaced_block_id(&self.id) {
            return Err("ID block phải có dạng <owner>/<slug> hợp lệ.");
        }
        let Some((owner_prefix, _)) = self.id.split_once('/') else {
            return Err("ID block thiếu tiền tố owner.");
        };
        if owner_prefix != expected_owner_prefix {
            return Err("ID block không khớp tiền tố owner của gói sở hữu.");
        }
        for constraint_list in [&self.allowed_parents, &self.allowed_children] {
            if let Some(items) = constraint_list {
                if items.is_empty() || items.len() > 32 {
                    return Err(
                        "allowed_parents / allowed_children phải có 1..=32 phần tử khi được khai báo.",
                    );
                }
                for item in items {
                    let valid_token = !item.is_empty()
                        && item.len() <= 80
                        && item.bytes().all(|b| {
                            b.is_ascii_lowercase()
                                || b.is_ascii_digit()
                                || matches!(b, b'_' | b'-' | b'/')
                        });
                    if !valid_token {
                        return Err(
                            "Phần tử trong allowed_parents / allowed_children chứa ký tự không hợp lệ.",
                        );
                    }
                }
            }
        }

        validate_field_specs(&self.fields)
    }

    pub fn validate_values(
        &self,
        values: &BTreeMap<String, Value>,
        previous_values: Option<&BTreeMap<String, Value>>,
    ) -> Result<(), &'static str> {
        validate_field_values(&self.fields, values, previous_values)
    }

    pub fn collect_media_ids(&self, values: &BTreeMap<String, Value>) -> CollectedBlockMediaIds {
        collect_field_media_ids(&self.fields, values)
    }

    pub fn extract_visible_text(&self, values: &BTreeMap<String, Value>, out: &mut Vec<String>) {
        for field in self.flattened_top_fields() {
            match field {
                BlockFieldSpec::Text { key, .. } | BlockFieldSpec::Textarea { key, .. } => {
                    if let Some(s) = values.get(key.as_str()).and_then(Value::as_str) {
                        let trimmed = s.trim();
                        if !trimmed.is_empty() {
                            out.push(trimmed.to_owned());
                        }
                    }
                }
                BlockFieldSpec::Richtext { key, .. } => {
                    if let Some(s) = values.get(key.as_str()).and_then(Value::as_str) {
                        let plain = strip_richtext_to_plain(s);
                        if !plain.is_empty() {
                            out.push(plain);
                        }
                    }
                }
                BlockFieldSpec::Repeater { key, fields, .. } => {
                    let Some(items) = values.get(key.as_str()).and_then(Value::as_array) else {
                        continue;
                    };
                    for item in items.iter().take(MAX_REPEATER_ITEMS) {
                        let Some(obj) = item.as_object() else {
                            continue;
                        };
                        for sub in fields {
                            match sub {
                                BlockFieldSpec::Text { key: sub_key, .. }
                                | BlockFieldSpec::Textarea { key: sub_key, .. } => {
                                    if let Some(s) =
                                        obj.get(sub_key.as_str()).and_then(Value::as_str)
                                    {
                                        let trimmed = s.trim();
                                        if !trimmed.is_empty() {
                                            out.push(trimmed.to_owned());
                                        }
                                    }
                                }
                                BlockFieldSpec::Richtext { key: sub_key, .. } => {
                                    if let Some(s) =
                                        obj.get(sub_key.as_str()).and_then(Value::as_str)
                                    {
                                        let plain = strip_richtext_to_plain(s);
                                        if !plain.is_empty() {
                                            out.push(plain);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn flattened_top_fields(&self) -> Vec<&BlockFieldSpec> {
        flattened_field_specs(&self.fields)
    }
}

pub fn flattened_field_specs(fields: &[BlockFieldSpec]) -> Vec<&BlockFieldSpec> {
    let mut out = Vec::new();
    for field in fields {
        if let BlockFieldSpec::Group { fields, .. } = field {
            out.extend(fields.iter());
        } else {
            out.push(field);
        }
    }
    out
}

pub fn validate_field_specs(fields: &[BlockFieldSpec]) -> Result<(), &'static str> {
    let mut seen_top_keys: BTreeSet<&str> = BTreeSet::new();
    let mut seen_css_vars: BTreeSet<&str> = BTreeSet::new();
    let mut top_real_fields: Vec<&BlockFieldSpec> = Vec::new();
    let mut group_keys: Vec<&str> = Vec::new();
    let mut group_rules: Vec<(&str, &ShowIfRule)> = Vec::new();

    for field in fields {
        if let BlockFieldSpec::Group {
            key,
            label,
            fields,
            show_if,
        } = field
        {
            if !valid_snake_key_with_limit(key, 32) {
                return Err("Khóa Group không hợp lệ.");
            }
            if key == "url" || key.ends_with("_url") {
                return Err("Khóa Group không được dùng tên 'url' hoặc hậu tố '_url'.");
            }
            validate_label(label)?;
            if !seen_top_keys.insert(key.as_str()) {
                return Err("Khóa field trùng lặp trong block.");
            }
            if fields.is_empty() {
                return Err("Group không được rỗng.");
            }
            group_keys.push(key.as_str());
            if let Some(rule) = show_if {
                group_rules.push((key.as_str(), rule));
            }
            for child in fields {
                if matches!(child, BlockFieldSpec::Group { .. }) {
                    return Err("Không cho phép lồng Group trong Group.");
                }
                if !seen_top_keys.insert(child.key()) {
                    return Err("Khóa field trùng lặp trong block.");
                }
                top_real_fields.push(child);
            }
        } else {
            if !seen_top_keys.insert(field.key()) {
                return Err("Khóa field trùng lặp trong block.");
            }
            top_real_fields.push(field);
        }
    }

    if top_real_fields.len() > MAX_TOP_REAL_FIELDS {
        return Err("Block vượt quá giới hạn tối đa 32 field thực top-level.");
    }

    let top_media_keys: BTreeSet<&str> = top_real_fields
        .iter()
        .filter_map(|f| match f {
            BlockFieldSpec::Media { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    let top_link_keys: BTreeSet<&str> = top_real_fields
        .iter()
        .filter_map(|f| match f {
            BlockFieldSpec::Link { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    let top_spacing_keys: BTreeSet<&str> = top_real_fields
        .iter()
        .filter_map(|f| match f {
            BlockFieldSpec::Spacing { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();

    for gkey in group_keys {
        for suffix in ["_url", "_alt", "_width", "_height"] {
            if let Some(prefix) = gkey.strip_suffix(suffix)
                && top_media_keys.contains(prefix)
            {
                return Err(
                    "Khóa Group không được trùng biến tổng hợp <media_key>_{url,alt,width,height}.",
                );
            }
        }
        for suffix in ["_url", "_target", "_rel", "_new_tab", "_nofollow"] {
            if let Some(prefix) = gkey.strip_suffix(suffix)
                && top_link_keys.contains(prefix)
            {
                return Err(
                    "Khóa Group không được trùng biến tổng hợp <link_key>_{url,target,rel,new_tab,nofollow}.",
                );
            }
        }
        if let Some(prefix) = gkey.strip_suffix("_classes")
            && top_spacing_keys.contains(prefix)
        {
            return Err("Khóa Group không được trùng biến tổng hợp <spacing_key>_classes.");
        }
    }

    let top_flat_map: BTreeMap<&str, &BlockFieldSpec> =
        top_real_fields.iter().map(|f| (f.key(), *f)).collect();

    let mut total_leaf_fields = 0usize;
    let mut total_repeater_max_items = 0usize;

    for field in &top_real_fields {
        check_scope_reserved_key(field, &top_media_keys, &top_link_keys, &top_spacing_keys)?;

        if let Some(rule) = field.show_if() {
            validate_show_if_rule(rule, Some(field.key()), &top_flat_map)?;
        }

        if let BlockFieldSpec::Repeater {
            label,
            min_items,
            max_items,
            item_label_field,
            fields,
            ..
        } = field
        {
            validate_label(label)?;
            if *max_items == 0 || *max_items > MAX_REPEATER_ITEMS || *min_items > *max_items {
                return Err("Số phần tử Repeater phải thỏa 1 <= max_items <= 32 và min <= max.");
            }
            total_repeater_max_items += *max_items;
            if total_repeater_max_items > MAX_TOTAL_REPEATER_ITEMS {
                return Err("Tổng max_items của các Repeater trong block vượt quá 64.");
            }
            if fields.is_empty() {
                return Err("Repeater bắt buộc có ít nhất 1 sub-field.");
            }

            let mut rep_keys: BTreeSet<&str> = BTreeSet::new();
            let mut rep_media_keys: BTreeSet<&str> = BTreeSet::new();
            let mut rep_link_keys: BTreeSet<&str> = BTreeSet::new();
            let mut rep_spacing_keys: BTreeSet<&str> = BTreeSet::new();
            let mut rep_sub_map: BTreeMap<&str, &BlockFieldSpec> = BTreeMap::new();

            for sub in fields {
                if matches!(
                    sub,
                    BlockFieldSpec::Group { .. } | BlockFieldSpec::Repeater { .. }
                ) {
                    return Err("Repeater chỉ được chứa các kiểu field nguyên thủy.");
                }
                if !rep_keys.insert(sub.key()) {
                    return Err("Khóa sub-field bị trùng trong Repeater.");
                }
                if matches!(sub, BlockFieldSpec::Media { .. }) {
                    rep_media_keys.insert(sub.key());
                }
                if matches!(sub, BlockFieldSpec::Link { .. }) {
                    rep_link_keys.insert(sub.key());
                }
                if matches!(sub, BlockFieldSpec::Spacing { .. }) {
                    rep_spacing_keys.insert(sub.key());
                }
                rep_sub_map.insert(sub.key(), sub);
            }

            for sub in fields {
                check_scope_reserved_key(sub, &rep_media_keys, &rep_link_keys, &rep_spacing_keys)?;
                if let Some(rule) = sub.show_if() {
                    validate_show_if_rule(rule, None, &top_flat_map)?;
                }
                validate_primitive_field(sub, true, &mut seen_css_vars)?;
                total_leaf_fields += 1;
            }

            if let Some(label_key) = item_label_field {
                let Some(target) = rep_sub_map.get(label_key.as_str()) else {
                    return Err("item_label_field không tồn tại trong Repeater.fields.");
                };
                if !matches!(
                    target,
                    BlockFieldSpec::Text { .. }
                        | BlockFieldSpec::Select { .. }
                        | BlockFieldSpec::RadioButtons { .. }
                        | BlockFieldSpec::Number { .. }
                        | BlockFieldSpec::Url { .. }
                ) {
                    return Err(
                        "item_label_field chỉ được trỏ tới Text/Select/RadioButtons/Number/Url.",
                    );
                }
            }
        } else {
            validate_primitive_field(field, false, &mut seen_css_vars)?;
            total_leaf_fields += 1;
        }
    }

    for (group_key, rule) in group_rules {
        validate_show_if_rule(rule, Some(group_key), &top_flat_map)?;
    }

    if total_leaf_fields > MAX_TOTAL_LEAF_FIELDS {
        return Err("Tổng số field lá của block (kể cả trong Repeater) vượt quá 48.");
    }

    Ok(())
}

pub fn validate_field_values(
    fields: &[BlockFieldSpec],
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
) -> Result<(), &'static str> {
    if values.len() > MAX_NODE_VALUES_KEYS {
        return Err("Số lượng khóa props.values của block vượt quá giới hạn 64.");
    }
    let serialized_len = serde_json::to_vec(values)
        .map_err(|_| "Không thể tuần tự hóa dữ liệu block.")?
        .len();
    if serialized_len > MAX_NODE_VALUES_BYTES {
        return Err("Kích thước dữ liệu props.values của block vượt quá 64 KiB.");
    }

    let top_flat_fields = flattened_field_specs(fields);
    let defaults_map: BTreeMap<&str, Option<Value>> = top_flat_fields
        .iter()
        .map(|f| (f.key(), f.default_json_value()))
        .collect();

    for field in fields {
        if let BlockFieldSpec::Group {
            fields, show_if, ..
        } = field
        {
            for child in fields {
                validate_top_entry(
                    child,
                    show_if.as_ref(),
                    values,
                    previous_values,
                    &defaults_map,
                )?;
            }
        } else {
            validate_top_entry(field, None, values, previous_values, &defaults_map)?;
        }
    }

    Ok(())
}

pub fn collect_field_media_ids(
    fields: &[BlockFieldSpec],
    values: &BTreeMap<String, Value>,
) -> CollectedBlockMediaIds {
    let mut out = CollectedBlockMediaIds::default();
    for field in flattened_field_specs(fields) {
        match field {
            BlockFieldSpec::Media {
                key, media_kind, ..
            } => {
                if let Some(val) = values.get(key.as_str()).and_then(Value::as_str)
                    && valid_sha256_media_id(val)
                {
                    push_collected_media(&mut out, *media_kind, val);
                }
            }
            BlockFieldSpec::Repeater { key, fields, .. } => {
                let Some(items) = values.get(key.as_str()).and_then(Value::as_array) else {
                    continue;
                };
                for item in items.iter().take(MAX_REPEATER_ITEMS) {
                    let Some(obj) = item.as_object() else {
                        continue;
                    };
                    for sub in fields {
                        if let BlockFieldSpec::Media {
                            key: sub_key,
                            media_kind,
                            ..
                        } = sub
                            && let Some(val) = obj.get(sub_key.as_str()).and_then(Value::as_str)
                            && valid_sha256_media_id(val)
                        {
                            push_collected_media(&mut out, *media_kind, val);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_collected_media(out: &mut CollectedBlockMediaIds, kind: MediaFieldKind, id: &str) {
    match kind {
        MediaFieldKind::Image => {
            out.image_ids.insert(id.to_owned());
        }
        MediaFieldKind::Video => {
            out.video_ids.insert(id.to_owned());
        }
        MediaFieldKind::Any => {
            out.any_ids.insert(id.to_owned());
        }
    }
}

fn validate_show_if_rule(
    rule: &ShowIfRule,
    self_key: Option<&str>,
    top_flat_map: &BTreeMap<&str, &BlockFieldSpec>,
) -> Result<(), &'static str> {
    if self_key.is_some_and(|k| k == rule.field) {
        return Err("show_if không được tự tham chiếu chính mình.");
    }
    let Some(target) = top_flat_map.get(rule.field.as_str()) else {
        return Err("show_if phải trỏ tới field top-level tồn tại.");
    };
    if matches!(
        target,
        BlockFieldSpec::Group { .. }
            | BlockFieldSpec::Repeater { .. }
            | BlockFieldSpec::Spacing { .. }
            | BlockFieldSpec::Link { .. }
    ) {
        return Err("show_if không được trỏ tới Group, Repeater, Spacing hoặc Link.");
    }
    if target.is_responsive() {
        return Err("show_if không được trỏ tới field có responsive: true.");
    }
    Ok(())
}

fn eval_show_if(
    rule: &ShowIfRule,
    values: &BTreeMap<String, Value>,
    defaults_map: &BTreeMap<&str, Option<Value>>,
) -> bool {
    let val = match values.get(rule.field.as_str()) {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => defaults_map.get(rule.field.as_str()).cloned().flatten(),
    };
    match (&rule.equals, val.as_ref()) {
        (ShowIfScalar::Null, None) | (ShowIfScalar::Null, Some(Value::Null)) => true,
        (ShowIfScalar::Bool(b), Some(Value::Bool(v))) => b == v,
        (ShowIfScalar::Int(i), Some(Value::Number(n))) => {
            n.as_i64() == Some(*i)
                || n.as_f64()
                    .is_some_and(|f| f.is_finite() && f == (*i as f64))
        }
        (ShowIfScalar::String(s), Some(Value::String(v))) => s == v,
        _ => false,
    }
}

fn validate_top_entry(
    field: &BlockFieldSpec,
    group_show_if: Option<&ShowIfRule>,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    defaults_map: &BTreeMap<&str, Option<Value>>,
) -> Result<(), &'static str> {
    let group_visible = group_show_if.is_none_or(|rule| eval_show_if(rule, values, defaults_map));
    let visible = group_visible
        && field
            .show_if()
            .is_none_or(|rule| eval_show_if(rule, values, defaults_map));

    let cur_val = values.get(field.key());
    let prev_visible = previous_values.is_some_and(|prev| {
        let prev_group_visible =
            group_show_if.is_none_or(|rule| eval_show_if(rule, prev, defaults_map));
        prev_group_visible
            && field
                .show_if()
                .is_none_or(|rule| eval_show_if(rule, prev, defaults_map))
    });
    let upgrade_exempt = previous_values.is_some_and(|prev| {
        prev_visible && !prev.contains_key(field.key()) && cur_val.is_none_or(Value::is_null)
    });

    if let BlockFieldSpec::Repeater {
        key,
        min_items,
        max_items,
        fields,
        ..
    } = field
    {
        match cur_val {
            None | Some(Value::Null) => {
                if visible && !upgrade_exempt && *min_items > 0 {
                    return Err("Repeater chưa đạt số phần tử tối thiểu min_items.");
                }
                return Ok(());
            }
            Some(Value::Array(items)) => {
                if items.len() > *max_items {
                    return Err("Repeater vượt quá số phần tử tối đa max_items.");
                }
                if visible && !upgrade_exempt && items.len() < *min_items {
                    return Err("Repeater chưa đạt số phần tử tối thiểu min_items.");
                }
                let prev_items = previous_values
                    .and_then(|p| p.get(key.as_str()))
                    .and_then(Value::as_array);
                for (idx, item) in items.iter().enumerate() {
                    let Some(obj) = item.as_object() else {
                        return Err("Mỗi phần tử trong Repeater phải là JSON object.");
                    };
                    if obj.len() > MAX_NODE_VALUES_KEYS {
                        return Err("Phần tử Repeater vượt quá 64 khóa.");
                    }
                    let prev_obj = prev_items
                        .and_then(|arr| arr.get(idx))
                        .and_then(Value::as_object);
                    for sub in fields {
                        let sub_visible = visible
                            && sub
                                .show_if()
                                .is_none_or(|rule| eval_show_if(rule, values, defaults_map));
                        let sub_cur = obj.get(sub.key());
                        let sub_prev_visible = prev_visible
                            && previous_values.is_some_and(|prev| {
                                sub.show_if()
                                    .is_none_or(|rule| eval_show_if(rule, prev, defaults_map))
                            });
                        let sub_upgrade_exempt = prev_obj.is_some_and(|prev_item| {
                            sub_prev_visible
                                && !prev_item.contains_key(sub.key())
                                && sub_cur.is_none_or(Value::is_null)
                        });
                        validate_leaf_value(sub, sub_cur, sub_visible && !sub_upgrade_exempt)?;
                    }
                }
                return Ok(());
            }
            _ => return Err("Giá trị Repeater phải là mảng JSON."),
        }
    }

    validate_leaf_value(field, cur_val, visible && !upgrade_exempt)
}

pub fn validate_single_field_value(
    field: &BlockFieldSpec,
    val: Option<&Value>,
    enforce_required: bool,
) -> Result<(), &'static str> {
    validate_leaf_value(field, val, enforce_required)
}

fn validate_leaf_value(
    field: &BlockFieldSpec,
    val: Option<&Value>,
    enforce_required: bool,
) -> Result<(), &'static str> {
    if field.is_responsive() {
        return validate_responsive_leaf(field, val, enforce_required);
    }
    match val {
        None | Some(Value::Null) => {
            if enforce_required && field_is_required(field) {
                return Err("Thiếu giá trị cho field bắt buộc.");
            }
            Ok(())
        }
        Some(v) => validate_scalar_leaf(field, v, enforce_required),
    }
}

fn validate_responsive_leaf(
    field: &BlockFieldSpec,
    val: Option<&Value>,
    enforce_required: bool,
) -> Result<(), &'static str> {
    match val {
        None | Some(Value::Null) => {
            if enforce_required && field_is_required(field) {
                return Err("Thiếu giá trị cho field responsive bắt buộc.");
            }
            Ok(())
        }
        Some(Value::Object(map)) => {
            let mut has_non_null = false;
            for (bp, inner) in map {
                if !matches!(bp.as_str(), "desktop" | "tablet" | "mobile") {
                    return Err("Khóa breakpoint responsive không hợp lệ.");
                }
                if !inner.is_null() {
                    has_non_null = true;
                    validate_scalar_leaf(field, inner, false)?;
                }
            }
            if enforce_required && field_is_required(field) && !has_non_null {
                return Err("Field responsive bắt buộc phải có ít nhất một giá trị breakpoint.");
            }
            Ok(())
        }
        Some(scalar) => validate_scalar_leaf(field, scalar, enforce_required),
    }
}

fn field_is_required(field: &BlockFieldSpec) -> bool {
    match field {
        BlockFieldSpec::Text { required, .. }
        | BlockFieldSpec::Textarea { required, .. }
        | BlockFieldSpec::Richtext { required, .. }
        | BlockFieldSpec::Number { required, .. }
        | BlockFieldSpec::Select { required, .. }
        | BlockFieldSpec::RadioButtons { required, .. }
        | BlockFieldSpec::Color { required, .. }
        | BlockFieldSpec::Media { required, .. }
        | BlockFieldSpec::Url { required, .. }
        | BlockFieldSpec::Link { required, .. } => *required,
        BlockFieldSpec::Range { .. }
        | BlockFieldSpec::Boolean { .. }
        | BlockFieldSpec::Spacing { .. }
        | BlockFieldSpec::Group { .. }
        | BlockFieldSpec::Repeater { .. } => false,
    }
}

fn validate_scalar_leaf(
    field: &BlockFieldSpec,
    val: &Value,
    enforce_required: bool,
) -> Result<(), &'static str> {
    match field {
        BlockFieldSpec::Text {
            max_chars,
            required,
            ..
        } => {
            let s = val.as_str().ok_or("Giá trị Text phải là chuỗi.")?;
            let limit = max_chars.unwrap_or(256);
            if s.chars().count() > limit {
                return Err("Giá trị Text vượt quá giới hạn ký tự.");
            }
            if enforce_required && *required && s.trim().is_empty() {
                return Err("Field Text bắt buộc không được để trống.");
            }
        }
        BlockFieldSpec::Textarea {
            max_chars,
            required,
            ..
        } => {
            let s = val.as_str().ok_or("Giá trị Textarea phải là chuỗi.")?;
            let limit = max_chars.unwrap_or(4000);
            if s.chars().count() > limit {
                return Err("Giá trị Textarea vượt quá giới hạn ký tự.");
            }
            if enforce_required && *required && s.trim().is_empty() {
                return Err("Field Textarea bắt buộc không được để trống.");
            }
        }
        BlockFieldSpec::Richtext {
            max_bytes,
            required,
            ..
        } => {
            let s = val.as_str().ok_or("Giá trị Richtext phải là chuỗi.")?;
            let cleaned = sanitize_richtext(s)?;
            let limit = max_bytes.unwrap_or(MAX_RICHTEXT_OUTPUT_BYTES);
            if cleaned.len() > limit {
                return Err("Giá trị Richtext vượt quá max_bytes sau làm sạch.");
            }
            if enforce_required && *required && strip_richtext_to_plain(&cleaned).trim().is_empty()
            {
                return Err("Field Richtext bắt buộc không được để trống.");
            }
        }
        BlockFieldSpec::Number { min, max, .. } => {
            let n = val.as_f64().ok_or("Giá trị Number phải là số.")?;
            if !n.is_finite() || min.is_some_and(|mn| n < mn) || max.is_some_and(|mx| n > mx) {
                return Err("Giá trị Number không hợp lệ hoặc nằm ngoài [min, max].");
            }
        }
        BlockFieldSpec::Range { min, max, .. } => {
            let n = val.as_f64().ok_or("Giá trị Range phải là số.")?;
            if !n.is_finite() || n < *min || n > *max {
                return Err("Giá trị Range nằm ngoài [min, max].");
            }
        }
        BlockFieldSpec::Boolean { .. } => {
            if !val.is_boolean() {
                return Err("Giá trị Boolean phải là true hoặc false.");
            }
        }
        BlockFieldSpec::Select { options, .. } | BlockFieldSpec::RadioButtons { options, .. } => {
            let s = val
                .as_str()
                .ok_or("Giá trị Select/RadioButtons phải là chuỗi.")?;
            if !options.iter().any(|opt| opt.value == s) {
                return Err("Giá trị Select/RadioButtons không nằm trong danh sách options.");
            }
        }
        BlockFieldSpec::Spacing { min, max, .. } => {
            let obj = val
                .as_object()
                .ok_or("Giá trị Spacing phải là JSON object {top, right, bottom, left}.")?;
            let lo = min.unwrap_or(0.0);
            let hi = max.unwrap_or(256.0);
            for (side_key, side_val) in obj {
                if !matches!(side_key.as_str(), "top" | "right" | "bottom" | "left") {
                    return Err("Khóa cạnh Spacing không hợp lệ.");
                }
                if !side_val.is_null() {
                    let n = side_val
                        .as_f64()
                        .ok_or("Giá trị cạnh Spacing phải là số.")?;
                    if !n.is_finite() || n < lo || n > hi {
                        return Err("Giá trị cạnh Spacing nằm ngoài [min, max].");
                    }
                }
            }
        }
        BlockFieldSpec::Color { required, .. } => {
            let s = val.as_str().ok_or("Giá trị Color phải là chuỗi.")?;
            if s.is_empty() {
                if enforce_required && *required {
                    return Err("Field Color bắt buộc không được để trống.");
                }
            } else if !valid_block_color(s) {
                return Err("Giá trị Color không hợp lệ.");
            }
        }
        BlockFieldSpec::Media { required, .. } => {
            let s = val.as_str().ok_or("Giá trị Media phải là chuỗi SHA-256.")?;
            if s.is_empty() {
                if enforce_required && *required {
                    return Err("Field Media bắt buộc không được để trống.");
                }
            } else if !valid_sha256_media_id(s) {
                return Err("Media ID trong block phải là SHA-256 64 ký tự hex.");
            }
        }
        BlockFieldSpec::Url { required, .. } => {
            let s = val.as_str().ok_or("Giá trị Url phải là chuỗi.")?;
            if s.is_empty() {
                if enforce_required && *required {
                    return Err("Field Url bắt buộc không được để trống.");
                }
            } else if !valid_block_url(s) {
                return Err("URL trong block không an toàn hoặc sai cú pháp.");
            }
        }
        BlockFieldSpec::Link { required, .. } => {
            let url_str = match val {
                Value::String(s) => s.as_str(),
                Value::Object(obj) => {
                    for (k, v) in obj {
                        match k.as_str() {
                            "url" => {
                                if !v.is_string() {
                                    return Err("Thuộc tính Link.url phải là chuỗi.");
                                }
                            }
                            "new_tab" | "nofollow" => {
                                if !v.is_boolean() {
                                    return Err(
                                        "Thuộc tính Link.{new_tab,nofollow} phải là boolean.",
                                    );
                                }
                            }
                            _ => return Err("Khóa không hợp lệ trong object Link."),
                        }
                    }
                    obj.get("url").and_then(Value::as_str).unwrap_or("")
                }
                _ => {
                    return Err(
                        "Giá trị Link phải là chuỗi URL hoặc object {url, new_tab, nofollow}.",
                    );
                }
            };
            if url_str.is_empty() {
                if enforce_required && *required {
                    return Err("Field Link bắt buộc không được để trống URL.");
                }
            } else if !valid_block_url(url_str) {
                return Err("URL trong field Link không an toàn hoặc sai cú pháp.");
            }
        }
        BlockFieldSpec::Group { .. } | BlockFieldSpec::Repeater { .. } => {
            return Err("Cấu trúc field lồng không hợp lệ.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_hex_id(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    #[test]
    fn validates_five_core_block_shapes() {
        let spacer: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/spacer",
            "title": "Khoảng trống",
            "category": "layout",
            "icon": "move-vertical",
            "render": "render.html",
            "fields": [
                {
                    "type": "range",
                    "key": "height",
                    "label": "Chiều cao",
                    "min": 8,
                    "max": 240,
                    "step": 4,
                    "default": 32,
                    "unit": "px",
                    "css_var": "--od-spacer-h",
                    "responsive": true
                }
            ]
        }))
        .unwrap();
        spacer.validate("core").unwrap();

        let banner: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/banner",
            "title": "Banner",
            "category": "media",
            "icon": "image",
            "render": "render.html",
            "fields": [
                {
                    "type": "group",
                    "key": "background",
                    "label": "Bố cục & Nền",
                    "fields": [
                        { "type": "media", "key": "bg_media", "label": "Ảnh nền", "media_kind": "image", "css_var": "--od-banner-bg" },
                        { "type": "color", "key": "overlay_color", "label": "Lớp phủ", "default": "transparent", "css_var": "--od-banner-overlay" },
                        { "type": "range", "key": "min_height", "label": "Chiều cao tối thiểu", "min": 160, "max": 800, "default": 360, "unit": "px", "responsive": true, "css_var": "--od-banner-h" }
                    ]
                },
                {
                    "type": "group",
                    "key": "content",
                    "label": "Nội dung & CTA",
                    "fields": [
                        {
                            "type": "select",
                            "key": "align",
                            "label": "Căn lề",
                            "options": [
                                { "value": "left", "label": "Trái" },
                                { "value": "center", "label": "Giữa" },
                                { "value": "right", "label": "Phải" }
                            ],
                            "default": "center",
                            "responsive": true,
                            "css_var": "--od-banner-align"
                        },
                        { "type": "text", "key": "heading", "label": "Tiêu đề", "required": true, "default": "Tiêu đề Banner" },
                        { "type": "richtext", "key": "body", "label": "Mô tả" },
                        { "type": "boolean", "key": "show_cta", "label": "Hiện nút CTA", "default": true },
                        { "type": "text", "key": "cta_label", "label": "Nhãn nút", "show_if": { "field": "show_cta", "equals": true }, "default": "Khám phá" },
                        { "type": "url", "key": "cta_url", "label": "Liên kết CTA", "show_if": { "field": "show_cta", "equals": true }, "default": "#" }
                    ]
                }
            ]
        }))
        .unwrap();
        banner.validate("core").unwrap();

        let accordion: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/accordion",
            "title": "Accordion / FAQ",
            "category": "interactive",
            "icon": "chevrons-up-down",
            "render": "render.html",
            "fields": [
                {
                    "type": "select",
                    "key": "style",
                    "label": "Kiểu hiển thị",
                    "options": [
                        { "value": "bordered", "label": "Có viền" },
                        { "value": "minimal", "label": "Tối giản" }
                    ],
                    "default": "bordered"
                },
                { "type": "boolean", "key": "open_first", "label": "Mở sẵn mục đầu", "default": true },
                {
                    "type": "repeater",
                    "key": "items",
                    "label": "Danh sách mục",
                    "min_items": 1,
                    "max_items": 24,
                    "item_label_field": "title",
                    "fields": [
                        { "type": "text", "key": "title", "label": "Tiêu đề", "required": true },
                        { "type": "richtext", "key": "content", "label": "Nội dung" },
                        { "type": "boolean", "key": "open", "label": "Mở sẵn", "default": false }
                    ]
                }
            ]
        }))
        .unwrap();
        accordion.validate("core").unwrap();
    }

    #[test]
    fn enforces_reserved_keys_and_media_url_collisions() {
        let non_url_with_url_key: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "my-theme/card",
            "title": "Card",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "text", "key": "url", "label": "Bad key" }
            ]
        }))
        .unwrap();
        assert!(non_url_with_url_key.validate("my-theme").is_err());

        for bad_synthetic_key in ["hero_url", "hero_alt", "hero_width", "hero_height"] {
            let field_type = if bad_synthetic_key == "hero_url" {
                "url"
            } else {
                "text"
            };
            let colliding: BlockSchemaV1 = serde_json::from_value(json!({
                "schema_version": 1,
                "id": "my-theme/card",
                "title": "Card",
                "category": "content",
                "render": "render.html",
                "fields": [
                    { "type": "media", "key": "hero", "label": "Hero", "media_kind": "image" },
                    { "type": field_type, "key": bad_synthetic_key, "label": "Collision" }
                ]
            }))
            .unwrap();
            assert!(
                colliding.validate("my-theme").is_err(),
                "expected collision error for {bad_synthetic_key}"
            );
        }

        // Top-level cta_url alongside media hero and layout min_height / border_width is valid
        let non_colliding: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "my-theme/card",
            "title": "Card",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "media", "key": "hero", "label": "Hero", "media_kind": "image" },
                { "type": "url", "key": "cta_url", "label": "CTA" },
                { "type": "range", "key": "min_height", "label": "Min H", "min": 100, "max": 800, "unit": "px" },
                { "type": "number", "key": "border_width", "label": "Border W", "min": 0, "max": 8, "unit": "px" }
            ]
        }))
        .unwrap();
        non_colliding.validate("my-theme").unwrap();
    }

    #[test]
    fn enforces_dos_bounds_units_colors_and_urls() {
        // Number with unit UI hint and css_var = None must be valid
        let num_hint: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/metric",
            "title": "Metric",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "number", "key": "gap", "label": "Khoảng cách", "min": 0, "max": 100, "unit": "px" }
            ]
        }))
        .unwrap();
        num_hint.validate("core").unwrap();

        // Nested repeater forbidden
        let nested_rep: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/nested",
            "title": "Nested",
            "category": "content",
            "render": "render.html",
            "fields": [
                {
                    "type": "repeater",
                    "key": "outer",
                    "label": "Outer",
                    "fields": [
                        {
                            "type": "repeater",
                            "key": "inner",
                            "label": "Inner",
                            "fields": [
                                { "type": "text", "key": "t", "label": "T" }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();
        assert!(nested_rep.validate("core").is_err());

        // > 32 real top-level fields rejected
        let too_many_top = BlockSchemaV1 {
            schema_version: 1,
            id: "core/big".into(),
            title: "Big".into(),
            category: BlockCategory::Content,
            icon: None,
            render: "render.html".into(),
            allowed_parents: None,
            allowed_children: None,
            fields: (0..33)
                .map(|i| BlockFieldSpec::Text {
                    key: format!("f_{i}"),
                    label: format!("Field {i}"),
                    max_chars: None,
                    required: false,
                    default: None,
                    placeholder: None,
                    show_if: None,
                })
                .collect(),
        };
        assert!(too_many_top.validate("core").is_err());

        // Total repeater max_items > 64 rejected
        let too_many_rep_items = BlockSchemaV1 {
            schema_version: 1,
            id: "core/many-rep".into(),
            title: "Many".into(),
            category: BlockCategory::Content,
            icon: None,
            render: "render.html".into(),
            allowed_parents: None,
            allowed_children: None,
            fields: (0..3)
                .map(|i| BlockFieldSpec::Repeater {
                    key: format!("r_{i}"),
                    label: format!("Rep {i}"),
                    min_items: 0,
                    max_items: 22,
                    item_label_field: None,
                    fields: vec![BlockFieldSpec::Text {
                        key: "t".into(),
                        label: "T".into(),
                        max_chars: None,
                        required: false,
                        default: None,
                        placeholder: None,
                        show_if: None,
                    }],
                    show_if: None,
                })
                .collect(),
        };
        assert!(too_many_rep_items.validate("core").is_err());

        // URLs & Colors matrix
        for ok_url in [
            "#",
            "#section-1_a",
            "/lien-he",
            "/san-pham?page=2#top",
            "http://example.com",
            "https://opendash.vn/docs",
            "mailto:hello@opendash.vn",
            "tel:+84901234567",
        ] {
            assert!(valid_block_url(ok_url), "expected ok url: {ok_url}");
        }
        for bad_url in [
            "",
            "//evil.com",
            "/a/../b",
            "/a\\b",
            "javascript:alert(1)",
            "data:text/html,hi",
            "vbscript:x",
            "https://user:pass@evil.com",
            "https://user@evil.com",
            "relative/no-slash",
            "#bad space",
            "https://evil.com/a\nb",
        ] {
            assert!(!valid_block_url(bad_url), "expected bad url: {bad_url}");
        }

        for tok in ALLOWED_COLOR_TOKENS {
            assert!(valid_block_color(tok));
        }
        assert!(valid_block_color("#fff"));
        assert!(valid_block_color("#123456"));
        assert!(valid_block_color("#123456cc"));
        assert!(valid_block_color("transparent"));
        assert!(valid_block_color("currentColor"));
        assert!(!valid_block_color("var(--od-token-unknown)"));
        assert!(!valid_block_color("rgb(0,0,0)"));
    }

    #[test]
    fn validates_values_with_show_if_upgrade_exemption_and_extraction() {
        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/feature",
            "title": "Feature",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "boolean", "key": "show_extra", "label": "Show Extra", "default": false },
                { "type": "text", "key": "extra_title", "label": "Extra Title", "required": true, "show_if": { "field": "show_extra", "equals": true } },
                { "type": "text", "key": "new_required", "label": "New Required", "required": true },
                { "type": "media", "key": "hero", "label": "Hero", "media_kind": "any" },
                { "type": "richtext", "key": "body", "label": "Body" },
                { "type": "range", "key": "pad", "label": "Pad", "min": 0, "max": 80, "responsive": true, "unit": "px" },
                {
                    "type": "repeater",
                    "key": "items",
                    "label": "Items",
                    "fields": [
                        { "type": "text", "key": "title", "label": "Title", "required": true },
                        { "type": "text", "key": "newly_required_sub", "label": "Sub", "required": true },
                        { "type": "media", "key": "thumb", "label": "Thumb", "media_kind": "image" },
                        { "type": "richtext", "key": "desc", "label": "Desc" }
                    ]
                }
            ]
        }))
        .unwrap();
        schema.validate("core").unwrap();

        let img_id = sample_hex_id('a');
        let any_id = sample_hex_id('b');

        // Previous values lack `new_required` and item[0].newly_required_sub -> upgrade exemption passes!
        // Also keeps `legacy_unknown_field` and hidden `extra_title` safely.
        let prev = BTreeMap::from([
            ("show_extra".into(), json!(false)),
            ("legacy_unknown_field".into(), json!("keep-me")),
            (
                "items".into(),
                json!([
                    { "title": "Item 1", "thumb": img_id, "desc": "<p>Chi tiết <b>1</b></p>" }
                ]),
            ),
        ]);
        let mut cur = prev.clone();
        cur.insert("hero".into(), json!(any_id));
        cur.insert("body".into(), json!("<p>Xin chào</p>"));
        cur.insert(
            "pad".into(),
            json!({ "desktop": 40, "tablet": 24, "mobile": 16 }),
        );

        schema.validate_values(&cur, Some(&prev)).unwrap();

        // Without previous_values (fresh element), missing `new_required` MUST fail
        assert!(schema.validate_values(&cur, None).is_err());

        // With show_extra = true and extra_title missing, fail
        let mut show_on = cur.clone();
        show_on.insert("show_extra".into(), json!(true));
        assert!(schema.validate_values(&show_on, Some(&prev)).is_err());

        // Bad responsive key fails
        let mut bad_bp = cur.clone();
        bad_bp.insert("pad".into(), json!({ "wide": 10 }));
        assert!(schema.validate_values(&bad_bp, Some(&prev)).is_err());

        // Media extraction & visible text extraction
        let media = schema.collect_media_ids(&cur);
        assert_eq!(media.image_ids, BTreeSet::from([img_id]));
        assert_eq!(media.any_ids, BTreeSet::from([any_id]));

        let mut texts = Vec::new();
        schema.extract_visible_text(&cur, &mut texts);
        assert_eq!(texts, vec!["Xin chào", "Item 1", "Chi tiết 1"]);
    }

    #[test]
    fn validates_spacing_radio_buttons_and_link_fields() {
        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "my-theme/cta-banner",
            "title": "CTA Banner",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                {
                    "type": "radio_buttons",
                    "key": "align",
                    "label": "Căn chữ",
                    "options": [
                        { "value": "left", "label": "Trái", "icon": "align-left" },
                        { "value": "center", "label": "Giữa", "icon": "align-center" },
                        { "value": "right", "label": "Phải", "icon": "align-right" }
                    ],
                    "default": "center",
                    "responsive": true,
                    "css_var": "--od-cta-align"
                },
                {
                    "type": "spacing",
                    "key": "pad",
                    "label": "Padding",
                    "min": 0,
                    "max": 120,
                    "unit": "px",
                    "default": { "top": 24, "right": 16, "bottom": 24, "left": 16 },
                    "responsive": true,
                    "css_var": "--od-cta-pad"
                },
                {
                    "type": "link",
                    "key": "cta",
                    "label": "Nút chính",
                    "required": true,
                    "default": { "url": "/lien-he", "new_tab": false, "nofollow": false }
                }
            ]
        }))
        .unwrap();
        schema.validate("my-theme").unwrap();

        let valid_vals = BTreeMap::from([
            (
                "align".into(),
                json!({ "desktop": "center", "mobile": "left" }),
            ),
            (
                "pad".into(),
                json!({ "desktop": { "top": 32, "right": 16, "bottom": 32, "left": 16 } }),
            ),
            (
                "cta".into(),
                json!({ "url": "https://opendash.vn", "new_tab": true, "nofollow": true }),
            ),
        ]);
        schema.validate_values(&valid_vals, None).unwrap();

        let bad_spacing = BTreeMap::from([
            ("pad".into(), json!({ "desktop": { "top": 500 } })),
            ("cta".into(), json!({ "url": "/ok" })),
        ]);
        assert!(schema.validate_values(&bad_spacing, None).is_err());

        let bad_link = BTreeMap::from([(
            "cta".into(),
            json!({ "url": "javascript:alert(1)", "new_tab": false }),
        )]);
        assert!(schema.validate_values(&bad_link, None).is_err());

        let colliding: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "my-theme/cta-banner",
            "title": "CTA",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                { "type": "link", "key": "cta", "label": "Link" },
                { "type": "text", "key": "cta_target", "label": "Bad collision" }
            ]
        }))
        .unwrap();
        assert!(colliding.validate("my-theme").is_err());

        let spacing_colliding: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "my-theme/cta-spacing",
            "title": "CTA Spacing",
            "category": "marketing",
            "render": "render.html",
            "fields": [
                { "type": "spacing", "key": "pad", "label": "Padding" },
                { "type": "text", "key": "pad_classes", "label": "Bad collision" }
            ]
        }))
        .unwrap();
        assert!(spacing_colliding.validate("my-theme").is_err());
    }
}
