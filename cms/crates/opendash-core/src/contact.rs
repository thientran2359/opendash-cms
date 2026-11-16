use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::id::valid_snake_key_with_limit;

pub const MAX_FORM_FIELDS: usize = 20;
pub const MAX_FIELD_KEY_BYTES: usize = 32;
pub const MAX_FIELD_LABEL_CHARS: usize = 120;
pub const MAX_TEXT_EMAIL_CHARS: usize = 254;
pub const MAX_TEXTAREA_CHARS: usize = 4000;
pub const MAX_SELECT_OPTIONS: usize = 20;
pub const MAX_SELECT_OPTION_CHARS: usize = 120;
pub const RESERVED_FIELD_KEYS: &[&str] = &["form_version", "nonce", "honeypot"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormKind {
    Text,
    Email,
    Textarea,
    Select,
    Checkbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub kind: FormKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormSchema {
    pub version: u32,
    pub fields: Vec<FormField>,
    #[serde(default)]
    pub reply_to_field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldValidationError {
    pub key: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSubmission {
    pub values: BTreeMap<String, serde_json::Value>,
    pub reply_to_email: Option<String>,
}

fn has_disallowed_control_chars(s: &str, allow_lf: bool) -> bool {
    s.chars().any(|c| {
        if allow_lf && c == '\n' {
            return false;
        }
        c.is_control()
    })
}

pub fn valid_ascii_email(addr: &str) -> bool {
    if addr.is_empty() || addr.len() > MAX_TEXT_EMAIL_CHARS || !addr.is_ascii() {
        return false;
    }
    let Some((local, domain)) = addr.split_once('@') else {
        return false;
    };
    if domain.contains('@') {
        return false;
    }
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return false;
    }
    let valid_atext = |b: u8| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'/'
                    | b'='
                    | b'?'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'{'
                    | b'|'
                    | b'}'
                    | b'~'
                    | b'.'
            )
    };
    if !local.bytes().all(valid_atext) {
        return false;
    }
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    for label in labels {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return false;
        }
    }
    true
}

pub fn validate_form_schema(schema: &FormSchema) -> Result<(), String> {
    if schema.version != 1 {
        return Err("Phiên bản schema form phải là 1.".to_string());
    }
    if schema.fields.is_empty() || schema.fields.len() > MAX_FORM_FIELDS {
        return Err(format!(
            "Form phải có từ 1 đến {MAX_FORM_FIELDS} trường."
        ));
    }
    let mut seen_keys = HashSet::new();
    let mut email_keys = HashSet::new();
    for field in &schema.fields {
        if !valid_snake_key_with_limit(&field.key, MAX_FIELD_KEY_BYTES) {
            return Err(format!("Mã trường '{}' không hợp lệ.", field.key));
        }
        if RESERVED_FIELD_KEYS.contains(&field.key.as_str()) {
            return Err(format!("Mã trường '{}' là từ khóa hệ thống.", field.key));
        }
        if !seen_keys.insert(field.key.as_str()) {
            return Err(format!("Mã trường '{}' bị trùng lặp.", field.key));
        }
        let label_trimmed = field.label.trim();
        if label_trimmed.is_empty()
            || field.label.chars().count() > MAX_FIELD_LABEL_CHARS
            || has_disallowed_control_chars(&field.label, false)
        {
            return Err(format!("Nhãn trường '{}' không hợp lệ.", field.key));
        }
        match field.kind {
            FormKind::Select => {
                let Some(opts) = field.options.as_ref() else {
                    return Err(format!("Trường select '{}' thiếu options.", field.key));
                };
                if opts.is_empty() || opts.len() > MAX_SELECT_OPTIONS {
                    return Err(format!(
                        "Trường select '{}' phải có từ 1 đến {MAX_SELECT_OPTIONS} lựa chọn.",
                        field.key
                    ));
                }
                let mut seen_opts = HashSet::new();
                for opt in opts {
                    let t = opt.trim();
                    if t.is_empty()
                        || opt.chars().count() > MAX_SELECT_OPTION_CHARS
                        || has_disallowed_control_chars(opt, false)
                    {
                        return Err(format!(
                            "Lựa chọn trong trường '{}' không hợp lệ.",
                            field.key
                        ));
                    }
                    if !seen_opts.insert(opt.as_str()) {
                        return Err(format!(
                            "Lựa chọn trong trường '{}' bị trùng lặp.",
                            field.key
                        ));
                    }
                }
            }
            _ => {
                if field.options.as_ref().is_some_and(|o| !o.is_empty()) {
                    return Err(format!(
                        "Chỉ trường select mới được khai báo options ('{}').",
                        field.key
                    ));
                }
            }
        }
        if field.kind == FormKind::Email {
            email_keys.insert(field.key.as_str());
        }
    }
    if let Some(reply_key) = schema.reply_to_field.as_deref() {
        if !email_keys.contains(reply_key) {
            return Err(
                "reply_to_field phải trỏ tới một trường kiểu email hợp lệ trong form."
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub fn validate_and_normalize_submission(
    schema: &FormSchema,
    pairs: &[(String, String)],
) -> Result<ValidatedSubmission, Vec<FieldValidationError>> {
    let mut errors = Vec::new();
    let allowed_keys: HashSet<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
    let mut incoming: BTreeMap<String, String> = BTreeMap::new();

    for (k, v) in pairs {
        if !allowed_keys.contains(k.as_str()) {
            errors.push(FieldValidationError {
                key: k.clone(),
                message: "Trường không tồn tại trong schema form.".to_string(),
            });
            continue;
        }
        if incoming.contains_key(k) {
            errors.push(FieldValidationError {
                key: k.clone(),
                message: "Trường bị gửi lặp lại.".to_string(),
            });
            continue;
        }
        incoming.insert(k.clone(), v.clone());
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut normalized = BTreeMap::new();
    let mut reply_to_email = None;

    for field in &schema.fields {
        let raw = incoming.get(&field.key).map(String::as_str);
        match field.kind {
            FormKind::Checkbox => {
                let checked = match raw {
                    None => false,
                    Some("true") => true,
                    Some(_) => {
                        errors.push(FieldValidationError {
                            key: field.key.clone(),
                            message: "Giá trị checkbox không hợp lệ.".to_string(),
                        });
                        continue;
                    }
                };
                if field.required && !checked {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Trường này là bắt buộc.".to_string(),
                    });
                    continue;
                }
                normalized.insert(field.key.clone(), serde_json::Value::Bool(checked));
            }
            FormKind::Text => {
                let val = raw.unwrap_or("").trim().to_string();
                if has_disallowed_control_chars(&val, false) {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Dữ liệu chứa ký tự điều khiển không hợp lệ.".to_string(),
                    });
                    continue;
                }
                if field.required && val.is_empty() {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Trường này là bắt buộc.".to_string(),
                    });
                    continue;
                }
                if val.chars().count() > MAX_TEXT_EMAIL_CHARS {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: format!("Tối đa {MAX_TEXT_EMAIL_CHARS} ký tự."),
                    });
                    continue;
                }
                normalized.insert(field.key.clone(), serde_json::Value::String(val));
            }
            FormKind::Email => {
                let val = raw.unwrap_or("").trim().to_string();
                if has_disallowed_control_chars(&val, false) {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Email chứa ký tự điều khiển không hợp lệ.".to_string(),
                    });
                    continue;
                }
                if field.required && val.is_empty() {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Email là bắt buộc.".to_string(),
                    });
                    continue;
                }
                if !val.is_empty() && !valid_ascii_email(&val) {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Địa chỉ email không hợp lệ.".to_string(),
                    });
                    continue;
                }
                if schema.reply_to_field.as_deref() == Some(field.key.as_str()) && !val.is_empty()
                {
                    reply_to_email = Some(val.clone());
                }
                normalized.insert(field.key.clone(), serde_json::Value::String(val));
            }
            FormKind::Textarea => {
                let norm = raw
                    .unwrap_or("")
                    .replace("\r\n", "\n")
                    .replace('\r', "\n");
                let trimmed = norm.trim().to_string();
                if has_disallowed_control_chars(&trimmed, true) {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Nội dung chứa ký tự điều khiển không hợp lệ.".to_string(),
                    });
                    continue;
                }
                if field.required && trimmed.is_empty() {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Trường này là bắt buộc.".to_string(),
                    });
                    continue;
                }
                if trimmed.chars().count() > MAX_TEXTAREA_CHARS {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: format!("Tối đa {MAX_TEXTAREA_CHARS} ký tự."),
                    });
                    continue;
                }
                normalized.insert(field.key.clone(), serde_json::Value::String(trimmed));
            }
            FormKind::Select => {
                let val = raw.unwrap_or("").to_string();
                if has_disallowed_control_chars(&val, false) {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Lựa chọn chứa ký tự điều khiển không hợp lệ.".to_string(),
                    });
                    continue;
                }
                if field.required && val.trim().is_empty() {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Vui lòng chọn một giá trị.".to_string(),
                    });
                    continue;
                }
                if !val.is_empty()
                    && !field
                        .options
                        .as_ref()
                        .is_some_and(|opts| opts.iter().any(|o| o == &val))
                {
                    errors.push(FieldValidationError {
                        key: field.key.clone(),
                        message: "Lựa chọn không nằm trong danh sách hợp lệ.".to_string(),
                    });
                    continue;
                }
                normalized.insert(field.key.clone(), serde_json::Value::String(val));
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(ValidatedSubmission {
        values: normalized,
        reply_to_email,
    })
}
