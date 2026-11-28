use super::*;

pub fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 100 && !value.chars().any(char::is_control)
}

/// A complete content entry write, with a stable slug scoped to its CPT.
/// Missing optional fields and explicit JSON null are both permitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentEntryInput {
    pub title: String,
    pub slug: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    /// Omitted on update: preserve existing membership (legacy client).
    /// Explicit []: unlink all. Kind/count validation lives in storage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term_ids: Option<Vec<i64>>,
}

/// Three-state patch field distinguishing omitted key (`Omitted`), JSON `null` (`Clear`), and value (`Set(T)`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PatchField<T> {
    #[default]
    Omitted,
    Clear,
    Set(T),
}

impl<T> PatchField<T> {
    pub fn is_omitted(&self) -> bool {
        matches!(self, Self::Omitted)
    }
}

impl<T: Serialize> Serialize for PatchField<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Omitted | Self::Clear => serializer.serialize_none(),
            Self::Set(val) => val.serialize(serializer),
        }
    }
}

pub fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        None => Ok(PatchField::Clear),
        Some(val) => Ok(PatchField::Set(val)),
    }
}

pub(super) fn validate_subgroups_for_panel(
    subgroups: &[LayoutFieldSubgroup],
    panel_field_keys: &[String],
    panel_id: &str,
) -> Result<(), LayoutValidationError> {
    if subgroups.len() > 12 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            format!("Panel '{panel_id}' vượt quá tối đa 12 subgroups."),
        ));
    }
    let allowed_set: HashSet<&str> = panel_field_keys.iter().map(String::as_str).collect();
    let mut seen_sg_ids = HashSet::new();
    let mut seen_fields = HashSet::new();
    for sg in subgroups {
        if !valid_snake_key(&sg.subgroup_id) || !seen_sg_ids.insert(sg.subgroup_id.as_str()) {
            return Err(LayoutValidationError::new(
                "invalid_subgroup",
                format!(
                    "subgroup_id '{}' không hợp lệ hoặc bị trùng.",
                    sg.subgroup_id
                ),
            ));
        }
        if !valid_label(&sg.title) {
            return Err(LayoutValidationError::new(
                "invalid_subgroup",
                format!("Tiêu đề subgroup '{}' không hợp lệ.", sg.subgroup_id),
            ));
        }
        for fk in &sg.field_keys {
            if !allowed_set.contains(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "unknown_field_in_subgroup",
                    format!(
                        "Trường '{fk}' trong subgroup '{}' không thuộc panel '{panel_id}'.",
                        sg.subgroup_id
                    ),
                ));
            }
            if !seen_fields.insert(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "duplicate_field_in_subgroups",
                    format!(
                        "Trường '{fk}' bị lặp lại trong nhiều subgroup của panel '{panel_id}'."
                    ),
                ));
            }
        }
    }
    Ok(())
}
