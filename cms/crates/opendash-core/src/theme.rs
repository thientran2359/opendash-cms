use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{
    valid_kebab_segment_with_limit, valid_namespaced_block_id, valid_namespaced_snake_key,
    valid_snake_key_with_limit,
};

pub type Fields = BTreeMap<String, String>;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BlockDescriptor {
    pub id: String,
    pub fields: Fields,
}

impl BlockDescriptor {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_namespaced_block_id(&self.id) {
            return Err("ID block không hợp lệ.");
        }
        if self.fields.is_empty()
            || self.fields.len() > 12
            || self.fields.iter().any(|(key, label)| {
                !valid_snake_key_with_limit(key, 32)
                    || label.trim().is_empty()
                    || label.chars().count() > 80
            })
        {
            return Err("Khai báo field không hợp lệ.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    #[serde(rename = "type")]
    pub kind: String,
    pub values: Fields,
}

impl Block {
    pub fn validate_against_fields(&self, fields: &Fields) -> Result<(), &'static str> {
        if self.values.keys().ne(fields.keys())
            || self
                .values
                .values()
                .any(|value| value.chars().count() > 512)
        {
            return Err("Giá trị nhập không khớp khai báo block.");
        }
        Ok(())
    }
}

pub fn valid_name(value: &str, separator: u8) -> bool {
    match separator {
        b'_' => valid_snake_key_with_limit(value, 32),
        b'-' => valid_kebab_segment_with_limit(value, 32),
        _ => {
            (1..=32).contains(&value.len())
                && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
                && value.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == separator
                })
        }
    }
}

/// A block key is namespaced, e.g. `core/heading`.
pub fn valid_block_key(value: &str) -> bool {
    valid_namespaced_snake_key(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_descriptor_and_values_validation() {
        let mut fields = Fields::new();
        fields.insert("title".into(), "Tiêu đề".into());
        fields.insert("note_1".into(), "Mô tả".into());

        let desc = BlockDescriptor {
            id: "sample-theme/hero-banner".into(),
            fields: fields.clone(),
        };
        assert!(desc.validate().is_ok());

        let bad_desc = BlockDescriptor {
            id: "Invalid/Block".into(),
            fields: fields.clone(),
        };
        assert!(bad_desc.validate().is_err());

        let mut values = Fields::new();
        values.insert("title".into(), "Xin chào".into());
        values.insert("note_1".into(), "Nội dung".into());
        let block = Block {
            kind: "sample-theme/hero-banner".into(),
            values,
        };
        assert!(block.validate_against_fields(&fields).is_ok());

        let empty_block = Block {
            kind: "sample-theme/hero-banner".into(),
            values: Fields::new(),
        };
        assert!(empty_block.validate_against_fields(&fields).is_err());
    }
}

#[cfg(test)]
mod scaffold_validation_tests {
    use super::valid_block_key;

    #[test]
    fn requires_valid_namespace_and_name() {
        assert!(valid_block_key("core/heading"));
        assert!(!valid_block_key("heading"));
        assert!(!valid_block_key("core/"));
        assert!(!valid_block_key("core/heading/extra"));
    }
}
