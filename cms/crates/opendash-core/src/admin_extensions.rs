//! Typed, bounded, declarative Admin extensions. No arbitrary scripts, CSS or HTML.
//! The installed package identity, never the manifest, determines ownership.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_COLUMNS_PER_OWNER: usize = 16;
pub const MAX_COLUMNS_EFFECTIVE: usize = 32;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AdminExtensions {
    pub schema_version: u8,
    #[serde(default)]
    pub columns: Vec<AdminColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnFormat {
    Text,
    Badge,
    Link,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AdminColumn {
    pub local_key: String,
    pub label: String,
    pub type_keys: Vec<String>,
    pub field_key: String,
    pub format: ColumnFormat,
    pub order: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

fn valid_local_key(key: &str) -> bool {
    opendash_key(key)
        && !matches!(
            key,
            "core"
                | "layout"
                | "title"
                | "slug"
                | "status"
                | "version"
                | "id"
                | "author"
                | "comments"
                | "trash"
                | "quick_edit"
                | "post"
                | "page"
                | "category"
                | "tag"
        )
        && !key.starts_with("core_")
        && !key.starts_with("layout_")
}

fn opendash_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && crate::id::valid_snake_key(key)
}

fn valid_label(label: &str) -> bool {
    let s = label.trim();
    !s.is_empty() && s.chars().count() <= 80 && !s.chars().any(char::is_control)
}

impl AdminExtensions {
    /// Validate before package publication and before registry mutation.
    /// Theme actions are forbidden; plugin action IDs must already be declared.
    pub fn validate(
        &self,
        owner_kind: &str,
        declared_plugin_actions: &[String],
    ) -> Result<(), &'static str> {
        if self.schema_version != 1 || !matches!(owner_kind, "theme" | "plugin") {
            return Err("admin_extensions chỉ hỗ trợ schema_version 1 và owner theme/plugin.");
        }
        if self.columns.len() > MAX_COLUMNS_PER_OWNER {
            return Err("admin_extensions vượt giới hạn 16 cột/owner.");
        }
        let mut seen = BTreeSet::new();
        for column in &self.columns {
            if !valid_local_key(&column.local_key) || !seen.insert(column.local_key.as_str()) {
                return Err("admin_extensions local_key trùng hoặc chiếm namespace core/layout.");
            }
            if !valid_label(&column.label) || !opendash_key(&column.field_key) {
                return Err("admin_extensions column label/field_key không hợp lệ.");
            }
            if column.type_keys.is_empty() || column.type_keys.len() > 32 {
                return Err("admin_extensions column type_keys phải có từ 1 đến 32 mục.");
            }
            let mut types = BTreeSet::new();
            for key in &column.type_keys {
                if !crate::content::valid_content_type_key(key) || !types.insert(key.as_str()) {
                    return Err("admin_extensions type_key không hợp lệ hoặc trùng.");
                }
            }
            if let Some(action_id) = &column.action_id {
                if owner_kind != "plugin"
                    || !declared_plugin_actions.iter().any(|id| id == action_id)
                {
                    return Err("Chỉ plugin được tham chiếu action_id đã khai báo.");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_core_namespaces_and_theme_action() {
        let mut value = AdminExtensions {
            schema_version: 1,
            columns: vec![AdminColumn {
                local_key: "core_title".into(),
                label: "Test".into(),
                type_keys: vec!["post".into()],
                field_key: "status_text".into(),
                format: ColumnFormat::Text,
                order: 0,
                action_id: None,
            }],
        };
        assert!(value.validate("plugin", &[]).is_err());
        value.columns[0].local_key = "provider_status".into();
        value.columns[0].action_id = Some("run".into());
        assert!(value.validate("theme", &[]).is_err());
        assert!(value.validate("plugin", &["run".into()]).is_ok());
        value.columns.push(value.columns[0].clone());
        assert!(value.validate("plugin", &["run".into()]).is_err());
    }
}
