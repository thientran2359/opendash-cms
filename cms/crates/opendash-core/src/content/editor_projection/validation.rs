use super::*;

pub fn valid_field_value(field: &crate::field_schema::FieldSpec, value: &Value) -> bool {
    let empty = BTreeMap::new();
    crate::field_schema::validate_field_value(
        field,
        crate::field_schema::FieldContext::Content,
        value,
        true,
        true,
        false,
        None,
        std::slice::from_ref(field),
        &empty,
        None,
        true,
    )
    .is_ok()
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
    existing: &[crate::field_schema::FieldSpec],
    incoming: &[crate::field_schema::FieldSpec],
    has_existing_entries: bool,
) -> Result<(), &'static str> {
    crate::field_schema::check_schema_compatibility(existing, incoming, has_existing_entries)
        .map_err(|_| "incompatible_schema_change: Không thể thay đổi schema không tương thích khi đã có dữ liệu.")
}
