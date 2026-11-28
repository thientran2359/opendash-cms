//! Clean entry body RichText extraction & sanitization up to 200 KiB for `core/entry-content`.

use super::*;

pub const MAX_ENTRY_BODY_RICHTEXT_BYTES: usize = 200 * 1024;

pub fn sanitize_entry_body_richtext(raw: &str) -> Result<String, &'static str> {
    opendash_core::richtext::sanitize_richtext_with_limits(
        raw,
        MAX_ENTRY_BODY_RICHTEXT_BYTES,
        MAX_ENTRY_BODY_RICHTEXT_BYTES,
    )
}

/// Extracts and sanitizes pure classic `body` RichText (if active, public, editor-readable,
/// and non-Builder). Returns `None` if `body` is absent, private, hidden, or Builder AST.
pub fn render_entry_body_fragment(
    values: &BTreeMap<String, Value>,
    effective_schema: &EffectiveContentSchema,
) -> Option<String> {
    use opendash_core::field_schema::FieldType;

    let body_fe = effective_schema.effective_fields.get("body")?;
    if body_fe.effective_status != EffectiveFieldStatus::Active
        || body_fe.definition.field_type != FieldType::Richtext
    {
        return None;
    }
    let (min_view_role, _, exposure) = body_fe.definition.effective_acl();
    if exposure != PublicExposure::Public || min_view_role > FieldRole::Editor {
        return None;
    }
    let raw = values.get("body").and_then(Value::as_str)?;
    if raw.trim().is_empty() || opendash_core::builder_blocks::contains_blocks(raw) {
        return None;
    }
    sanitize_entry_body_richtext(raw)
        .ok()
        .filter(|clean| !clean.trim().is_empty())
}
