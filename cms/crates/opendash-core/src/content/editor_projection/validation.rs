use super::*;

pub fn valid_field_value(field: &ContentFieldDefinition, value: &Value) -> bool {
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
            let len_ok = if crate::builder_blocks::contains_blocks(s) {
                s.len() <= crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES
                    && s.chars().count() <= crate::builder_blocks::MAX_BUILDER_BLOCK_BODY_BYTES
            } else {
                s.chars().count() <= 200_000
            };
            len_ok && (!field.required || !s.trim().is_empty())
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

/// The domain checks opaque media identifier syntax; the server storage layer
/// additionally verifies each image/gallery ID exists before saving entries.
pub fn valid_media_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.bytes().all(|c| {
            c.is_ascii_lowercase()
                || c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || c == b'-'
                || c == b'_'
        })
}

pub(crate) fn valid_iso_date(value: &str) -> bool {
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

/// Compatibility helper used by legacy callers.
pub fn check_base_fields_compatibility(
    existing: &[ContentFieldDefinition],
    incoming: &[ContentFieldDefinition],
    has_existing_entries: bool,
) -> Result<(), &'static str> {
    let incoming_by_key: HashMap<&str, &ContentFieldDefinition> =
        incoming.iter().map(|f| (f.key.as_str(), f)).collect();
    let existing_by_key: HashMap<&str, &ContentFieldDefinition> =
        existing.iter().map(|f| (f.key.as_str(), f)).collect();

    for old in existing {
        let Some(new) = incoming_by_key.get(old.key.as_str()) else {
            if has_existing_entries {
                return Err(
                    "cannot_remove_field_with_data: Không thể xóa hoặc đổi key field khi đã có dữ liệu cũ.",
                );
            }
            continue;
        };
        if old.field_type != new.field_type {
            return Err(
                "incompatible_schema_change: Không thể đổi kiểu dữ liệu (type) của custom field đã tồn tại.",
            );
        }
        let old_req = old.required && old.declaration_state != DeclarationState::Withdrawn;
        let new_req = new.required && new.declaration_state != DeclarationState::Withdrawn;
        if !old_req && new_req && has_existing_entries {
            return Err(
                "incompatible_schema_change: Không thể đổi field tùy chọn thành bắt buộc (required) khi đã có dữ liệu.",
            );
        }
        if old.field_type == ContentFieldType::Select && has_existing_entries {
            let new_values: HashSet<&str> = new.options.iter().map(|o| o.value.as_str()).collect();
            for old_opt in &old.options {
                if !new_values.contains(old_opt.value.as_str()) {
                    return Err(
                        "incompatible_schema_change: Không thể xóa lựa chọn (select option) khi đã có dữ liệu cũ.",
                    );
                }
            }
        }
    }

    for new in incoming {
        let new_req = new.required && new.declaration_state != DeclarationState::Withdrawn;
        if !existing_by_key.contains_key(new.key.as_str()) && new_req && has_existing_entries {
            return Err(
                "incompatible_schema_change: Không thể thêm field bắt buộc (required) mới khi đã có dữ liệu.",
            );
        }
    }

    Ok(())
}
