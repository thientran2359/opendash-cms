//! Content-type and custom-field schema for the CMS content module (M3).
//! Identifier rules come from `crate::id`; resource ownership is recorded by
//! wrapping a definition in `registry::OwnedDescriptor<ContentTypeDefinition>`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::{valid_kebab_segment_with_limit, valid_snake_key};

/// Valid identifiers start with a lowercase ASCII letter and contain only
/// lowercase letters, digits, or underscores (`1..=64` chars).
pub fn valid_content_type_key(value: &str) -> bool {
    valid_snake_key(value)
}

/// The nine custom-field kinds supported by the V1 content editor.
/// Rendering of field values belongs to later M3/M4 slices. Values are
/// validated here before persistence; rich text is never rendered as HTML here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentFieldType {
    ShortText,
    LongText,
    RichText,
    Number,
    Boolean,
    Date,
    Select,
    Image,
    Gallery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentFieldDefinition {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: ContentFieldType,
    #[serde(default)]
    pub required: bool,
    /// Used only for `select`. Other field kinds must not declare options.
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

impl ContentFieldDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_snake_key(&self.key) {
            return Err("Khóa custom field không hợp lệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên custom field không hợp lệ.");
        }
        if self.field_type != ContentFieldType::Select {
            if !self.options.is_empty() {
                return Err("Chỉ field lựa chọn mới được khai báo options.");
            }
            return Ok(());
        }
        if self.options.is_empty() || self.options.len() > 100 {
            return Err("Field lựa chọn cần từ 1 đến 100 phương án.");
        }
        let mut values = HashSet::new();
        for option in &self.options {
            if option.value.trim() != option.value
                || option.value.is_empty()
                || option.value.chars().count() > 128
                || option.value.chars().any(char::is_control)
                || !valid_label(&option.label)
                || !values.insert(option.value.as_str())
            {
                return Err("Phương án lựa chọn không hợp lệ hoặc trùng giá trị.");
            }
        }
        Ok(())
    }
}

/// Schema is independent of the theme/plugin that declared it. Ownership and
/// provider status are managed by the existing `RegistryCatalog` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDefinition {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<ContentFieldDefinition>,
}

impl ContentTypeDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa loại nội dung không hợp lệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên loại nội dung không hợp lệ.");
        }
        if self.fields.len() > 64 {
            return Err("Mỗi loại nội dung hỗ trợ tối đa 64 custom fields.");
        }
        let mut keys = HashSet::new();
        for field in &self.fields {
            field.validate()?;
            if !keys.insert(field.key.as_str()) {
                return Err("Trùng khóa custom field trong loại nội dung.");
            }
        }
        Ok(())
    }
}

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

/// A complete content entry write, with a stable slug scoped to its CPT.
/// Missing optional fields and explicit JSON null are both permitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentEntryInput {
    pub title: String,
    pub slug: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

impl ContentEntryInput {
    pub fn validate_against(&self, schema: &ContentTypeDefinition) -> Result<(), &'static str> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 200
            || self.title.chars().any(char::is_control)
        {
            return Err("Tiêu đề nội dung không hợp lệ.");
        }
        if !valid_kebab_segment_with_limit(&self.slug, 64) {
            return Err("Slug nội dung không hợp lệ.");
        }
        for key in self.values.keys() {
            if !schema.fields.iter().any(|field| field.key == *key) {
                return Err("Giá trị chứa custom field chưa được khai báo.");
            }
        }
        for field in &schema.fields {
            let value = self.values.get(&field.key).unwrap_or(&Value::Null);
            if value.is_null() {
                if field.required {
                    return Err("Thiếu custom field bắt buộc.");
                }
                continue;
            }
            if !valid_field_value(field, value) {
                return Err("Giá trị custom field không đúng kiểu hoặc vượt giới hạn.");
            }
        }
        Ok(())
    }
}

fn valid_field_value(field: &ContentFieldDefinition, value: &Value) -> bool {
    let text = value.as_str();
    match field.field_type {
        ContentFieldType::ShortText => text.is_some_and(|s| {
            s.chars().count() <= 512
                && s.chars().all(|c| !c.is_control())
                && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::LongText => text.is_some_and(|s| {
            s.chars().count() <= 100_000 && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::RichText => text.is_some_and(|s| {
            s.chars().count() <= 200_000 && (!field.required || !s.trim().is_empty())
        }),
        ContentFieldType::Number => value.is_number(),
        ContentFieldType::Boolean => value.is_boolean(),
        ContentFieldType::Date => text.is_some_and(valid_iso_date),
        ContentFieldType::Select => {
            text.is_some_and(|selected| field.options.iter().any(|option| option.value == selected))
        }
        ContentFieldType::Image => text.is_some_and(valid_media_id),
        ContentFieldType::Gallery => value.as_array().is_some_and(|items| {
            (!field.required || !items.is_empty()) && items.len() <= 50 && {
                let mut seen = HashSet::new();
                items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|id| valid_media_id(id) && seen.insert(id))
                })
            }
        }),
    }
}

/// Provisional opaque media identifiers; existence/permissions will be checked
/// when M3 Media Library is implemented, not by this domain-only validator.
fn valid_media_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|c| {
            c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == b'-'
                || c == b'_'
        })
}

fn valid_iso_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days_in_month
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_custom_post_type_keys() {
        assert!(valid_content_type_key("post"));
        assert!(valid_content_type_key("portfolio_item2"));
    }

    #[test]
    fn rejects_unstable_or_invalid_keys() {
        assert!(!valid_content_type_key(""));
        assert!(!valid_content_type_key("2post"));
        assert!(!valid_content_type_key("Post"));
        assert!(!valid_content_type_key("post/type"));
    }

    fn sample_schema() -> ContentTypeDefinition {
        ContentTypeDefinition {
            key: "portfolio_item".into(),
            label: "Dự án".into(),
            fields: vec![
                ContentFieldDefinition {
                    key: "summary".into(),
                    label: "Tóm tắt".into(),
                    field_type: ContentFieldType::LongText,
                    required: false,
                    options: vec![],
                },
                ContentFieldDefinition {
                    key: "status".into(),
                    label: "Trạng thái".into(),
                    field_type: ContentFieldType::Select,
                    required: true,
                    options: vec![
                        SelectOption {
                            value: "planned".into(),
                            label: "Dự kiến".into(),
                        },
                        SelectOption {
                            value: "published".into(),
                            label: "Đã đăng".into(),
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn content_schema_round_trips_and_validates() {
        let schema = sample_schema();
        assert!(schema.validate().is_ok());
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("\"type\":\"long_text\""));
        let decoded: ContentTypeDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, schema);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn supports_all_nine_v1_field_kinds() {
        let kinds = [
            ContentFieldType::ShortText,
            ContentFieldType::LongText,
            ContentFieldType::RichText,
            ContentFieldType::Number,
            ContentFieldType::Boolean,
            ContentFieldType::Date,
            ContentFieldType::Select,
            ContentFieldType::Image,
            ContentFieldType::Gallery,
        ];
        for kind in kinds {
            let mut field = sample_schema().fields[0].clone();
            field.field_type = kind;
            if kind == ContentFieldType::Select {
                field.options = sample_schema().fields[1].options.clone();
            }
            assert!(field.validate().is_ok());
        }
    }

    #[test]
    fn rejects_duplicate_keys_and_invalid_select_options() {
        let mut schema = sample_schema();
        schema.fields.push(schema.fields[0].clone());
        assert!(schema.validate().is_err());

        let mut schema = sample_schema();
        schema.fields[1].options[1].value = "planned".into();
        assert!(schema.validate().is_err());

        schema.fields[1].options.clear();
        assert!(schema.validate().is_err());

        schema.fields[0].options.push(SelectOption {
            value: "wrong".into(),
            label: "Sai loại".into(),
        });
        assert!(schema.validate().is_err());
    }

    #[test]
    fn rejects_invalid_identifiers_and_labels() {
        let mut schema = sample_schema();
        schema.key = "Invalid-Type".into();
        assert!(schema.validate().is_err());

        schema.key = "post".into();
        schema.label = "   ".into();
        assert!(schema.validate().is_err());

        schema.label = "Bài viết".into();
        schema.fields[0].key = "bad-name".into();
        assert!(schema.validate().is_err());
    }

    fn entry() -> ContentEntryInput {
        ContentEntryInput {
            title: "Dự án đầu tiên".into(),
            slug: "du-an-dau-tien".into(),
            values: BTreeMap::from([
                ("summary".into(), json!("Nội dung")),
                ("status".into(), json!("planned")),
            ]),
        }
    }

    #[test]
    fn validates_entry_and_all_nine_field_types() {
        let mut schema = sample_schema();
        let cases = [
            ("headline", ContentFieldType::ShortText, json!("Hello")),
            ("body", ContentFieldType::RichText, json!("<b>Text</b>")),
            ("budget", ContentFieldType::Number, json!(18.5)),
            ("featured", ContentFieldType::Boolean, json!(false)),
            ("due_date", ContentFieldType::Date, json!("2024-02-29")),
            ("photo", ContentFieldType::Image, json!("asset_1")),
            (
                "album",
                ContentFieldType::Gallery,
                json!(["asset_1", "asset_2"]),
            ),
        ];
        let mut entry = entry();
        for (key, kind, value) in cases {
            schema.fields.push(ContentFieldDefinition {
                key: key.into(),
                label: "Trường thử".into(),
                field_type: kind,
                required: false,
                options: vec![],
            });
            entry.values.insert(key.into(), value);
        }
        assert!(entry.validate_against(&schema).is_ok());
        let encoded = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            serde_json::from_str::<ContentEntryInput>(&encoded).unwrap(),
            entry
        );
    }

    #[test]
    fn rejects_invalid_custom_field_values_and_missing_required() {
        let schema = sample_schema();
        let mut entry = entry();
        entry.values.remove("status");
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("status".into(), json!("unknown"));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("status".into(), json!("planned"));
        entry.values.insert("unregistered".into(), json!("bad"));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.remove("unregistered");
        entry.slug = "Invalid_Slug".into();
        assert!(entry.validate_against(&schema).is_err());
        entry.slug = "valid-slug".into();
        entry.values.insert("summary".into(), json!(true));
        assert!(entry.validate_against(&schema).is_err());
    }

    #[test]
    fn rejects_invalid_dates_galleries_and_media_ids() {
        let mut schema = sample_schema();
        schema.fields.push(ContentFieldDefinition {
            key: "date".into(),
            label: "Ngày".into(),
            field_type: ContentFieldType::Date,
            required: true,
            options: vec![],
        });
        schema.fields.push(ContentFieldDefinition {
            key: "photos".into(),
            label: "Album".into(),
            field_type: ContentFieldType::Gallery,
            required: false,
            options: vec![],
        });
        let mut entry = entry();
        for bad in ["2025-02-29", "2024-13-01", "2024-02-30", "0000-01-01"] {
            entry.values.insert("date".into(), json!(bad));
            assert!(entry.validate_against(&schema).is_err(), "{bad}");
        }
        entry.values.insert("date".into(), json!("2000-02-29"));
        entry
            .values
            .insert("photos".into(), json!(["asset1", "asset1"]));
        assert!(entry.validate_against(&schema).is_err());
        entry.values.insert("photos".into(), json!(["../asset"]));
        assert!(entry.validate_against(&schema).is_err());
        entry
            .values
            .insert("photos".into(), json!(["asset1", "asset2"]));
        assert!(entry.validate_against(&schema).is_ok());
    }
}
