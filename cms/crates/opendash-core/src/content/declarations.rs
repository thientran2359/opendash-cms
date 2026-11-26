use super::*;

/// Retains omitted fields from `existing_fields` as `DeclarationState::Withdrawn` when a package
/// re-declares a CPT or field group (`270-R1`). Preserves declared order of incoming fields and
/// appends withdrawn fields deterministically.
pub fn merge_package_redeclared_fields(
    existing_fields: &[ContentFieldDefinition],
    incoming_fields: &[ContentFieldDefinition],
) -> Vec<ContentFieldDefinition> {
    let incoming_keys: HashSet<&str> = incoming_fields.iter().map(|f| f.key.as_str()).collect();
    let mut merged: Vec<ContentFieldDefinition> = incoming_fields.to_vec();
    for old in existing_fields {
        if !incoming_keys.contains(old.key.as_str()) {
            let mut withdrawn = old.clone();
            withdrawn.declaration_state = DeclarationState::Withdrawn;
            merged.push(withdrawn);
        }
    }
    merged
}

/// Syntax shared by CPT and taxonomy rewrite defaults. Route root reservations,
/// namespace collisions, and site overrides belong to the C3 storage resolver.
pub(super) fn validate_provider_rewrite_base(base: Option<&str>) -> Result<(), &'static str> {
    if let Some(base) = base {
        let segments: Vec<&str> = base.split('/').collect();
        if segments.len() > 3
            || segments
                .iter()
                .any(|segment| !valid_kebab_segment_with_limit(segment, 64))
        {
            return Err("rewrite_base phải có từ 1 đến 3 đoạn kebab-case hợp lệ.");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeDeclaration {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub fields: Vec<ContentFieldDefinition>,
    #[serde(default)]
    pub taxonomies: Vec<String>,
    /// Omission preserves existing provider-owned settings on package re-install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ContentTypeSettings>,
    /// Provider default only. The site's explicit C3 override takes precedence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewrite_base: Option<String>,
}
