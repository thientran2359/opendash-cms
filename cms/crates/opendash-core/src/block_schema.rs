use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    field_schema::{
        CollectedMediaIds, FieldContext, FieldSpec, SelectOptionsSource,
        collect_media_ids as collect_unified_media_ids, collect_options_source_values,
        extract_visible_text as extract_unified_visible_text, validate_field_specs,
        validate_field_values,
    },
    id::valid_namespaced_block_id,
};

pub const MAX_NODE_VALUES_KEYS: usize = 64;
pub const MAX_NODE_VALUES_BYTES: usize = 65_536;

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
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredBlockSchema {
    pub schema: BlockSchemaV1,
}

fn valid_icon_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 32
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
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

        validate_field_specs(&self.fields, FieldContext::StudioBlock)
    }

    pub fn validate_values(
        &self,
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

        validate_field_values(
            &self.fields,
            FieldContext::StudioBlock,
            values,
            previous_values,
        )
    }

    pub fn collect_media_ids(&self, values: &BTreeMap<String, Value>) -> CollectedMediaIds {
        collect_unified_media_ids(&self.fields, values)
    }

    pub fn extract_visible_text(&self, values: &BTreeMap<String, Value>, out: &mut Vec<String>) {
        extract_unified_visible_text(&self.fields, values, out);
    }

    pub fn collect_pattern_ids(&self, values: &BTreeMap<String, Value>) -> BTreeSet<i64> {
        collect_options_source_values(&self.fields, values, SelectOptionsSource::Patterns)
            .into_iter()
            .filter_map(|raw| raw.parse::<i64>().ok().filter(|&id| id > 0))
            .collect()
    }

    pub fn has_non_empty_pattern_source_value(&self, values: &BTreeMap<String, Value>) -> bool {
        !collect_options_source_values(&self.fields, values, SelectOptionsSource::Patterns)
            .is_empty()
    }

    pub fn collect_template_part_keys(&self, values: &BTreeMap<String, Value>) -> Vec<String> {
        collect_options_source_values(&self.fields, values, SelectOptionsSource::TemplateParts)
    }

    pub fn has_active_contact_form(&self, values: &BTreeMap<String, Value>) -> bool {
        !collect_options_source_values(&self.fields, values, SelectOptionsSource::ContactForms)
            .is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_hex_id(fill: char) -> String {
        std::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn validates_unified_v1_block_shapes_and_rejects_legacy_syntax() {
        let showcase: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/showcase",
            "title": "Showcase",
            "category": "content",
            "icon": "sparkles",
            "render": "render.html",
            "fields": [
                { "type": "tab", "key": "tab_main", "label": "Nội dung" },
                { "type": "message", "key": "hint", "label": "Mẹo", "body": "Dùng unified fields." },
                {
                    "type": "group",
                    "key": "hero_card",
                    "label": "Hero Card",
                    "fields": [
                        { "type": "media", "key": "cover_image", "label": "Cover", "media_kind": "image" },
                        { "type": "link", "key": "primary_link", "label": "Link", "default": { "url": "/gioi-thieu", "label": "Xem", "new_tab": false, "nofollow": false } },
                        { "type": "icon", "key": "badge_icon", "label": "Badge Icon", "default": "sparkles" }
                    ]
                },
                {
                    "type": "select",
                    "key": "linked_menu",
                    "label": "Menu",
                    "options_source": "menus",
                    "allow_null": true
                },
                {
                    "type": "select",
                    "key": "target_tax",
                    "label": "Taxonomy",
                    "options_source": "taxonomies"
                },
                {
                    "type": "term_ref",
                    "key": "dynamic_terms",
                    "label": "Terms",
                    "taxonomy_from_field": "target_tax",
                    "multiple": true
                },
                {
                    "type": "entry_ref",
                    "key": "related_any",
                    "label": "Related",
                    "target_types": ["*"],
                    "multiple": true
                },
                {
                    "type": "spacing",
                    "key": "outer_margin",
                    "label": "Margin",
                    "property": "margin",
                    "min": -64,
                    "max": 128,
                    "unit": "px",
                    "responsive": true
                },
                {
                    "type": "repeater",
                    "key": "highlights",
                    "label": "Highlights",
                    "min_items": 1,
                    "max_items": 8,
                    "fields": [
                        { "type": "text", "key": "title", "label": "Title", "required": true },
                        {
                            "type": "group",
                            "key": "meta",
                            "label": "Meta",
                            "fields": [
                                { "type": "icon", "key": "item_icon", "label": "Icon" }
                            ]
                        }
                    ]
                }
            ]
        }))
        .unwrap();
        showcase.validate("core").unwrap();

        // Owner prefix & malformed schema rejected
        assert!(showcase.validate("my-theme").is_err());

        // Legacy `show_if` rejected by #[serde(deny_unknown_fields)]
        let legacy_show_if = json!({
            "schema_version": 1,
            "id": "core/legacy",
            "title": "Legacy",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "boolean", "key": "flag", "label": "Flag" },
                { "type": "text", "key": "cta", "label": "CTA", "show_if": { "field": "flag", "equals": true } }
            ]
        });
        assert!(serde_json::from_value::<BlockSchemaV1>(legacy_show_if).is_err());

        // responsive: true inside nested group rejected by field_schema StudioBlock rules
        let responsive_in_group: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/bad-group",
            "title": "Bad Group",
            "category": "content",
            "render": "render.html",
            "fields": [
                {
                    "type": "group",
                    "key": "grp",
                    "label": "Group",
                    "fields": [
                        { "type": "range", "key": "pad", "label": "Pad", "min": 0, "max": 32, "responsive": true }
                    ]
                }
            ]
        }))
        .unwrap();
        assert!(responsive_in_group.validate("core").is_err());

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
                .map(|i| {
                    FieldSpec::new(
                        crate::field_schema::FieldType::Text,
                        format!("f_{i}"),
                        format!("Field {i}"),
                    )
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
                .map(|i| {
                    let mut rep = FieldSpec::new(
                        crate::field_schema::FieldType::Repeater,
                        format!("r_{i}"),
                        format!("Rep {i}"),
                    );
                    rep.max_items = Some(22);
                    rep.fields = vec![FieldSpec::new(
                        crate::field_schema::FieldType::Text,
                        "t",
                        "T",
                    )];
                    rep
                })
                .collect(),
        };
        assert!(too_many_rep_items.validate("core").is_err());
    }

    #[test]
    fn validates_values_with_dnf_conditions_upgrade_exemption_and_extraction() {
        let schema: BlockSchemaV1 = serde_json::from_value(json!({
            "schema_version": 1,
            "id": "core/feature",
            "title": "Feature",
            "category": "content",
            "render": "render.html",
            "fields": [
                { "type": "boolean", "key": "show_extra", "label": "Show Extra", "default": false },
                {
                    "type": "text",
                    "key": "extra_title",
                    "label": "Extra Title",
                    "required": true,
                    "conditions": [[{ "field": "show_extra", "op": "eq", "value": true }]]
                },
                { "type": "text", "key": "new_required", "label": "New Required", "required": true },
                { "type": "media", "key": "hero", "label": "Hero", "media_kind": "any" },
                { "type": "media", "key": "manual", "label": "Manual", "media_kind": "file" },
                { "type": "richtext", "key": "body", "label": "Body" },
                { "type": "select", "key": "pat", "label": "Pattern", "options_source": "patterns" },
                { "type": "select", "key": "part", "label": "Part", "options_source": "template_parts" },
                { "type": "select", "key": "form", "label": "Form", "options_source": "contact_forms" },
                { "type": "range", "key": "pad", "label": "Pad", "min": 0, "max": 80, "responsive": true, "unit": "px" },
                {
                    "type": "group",
                    "key": "card",
                    "label": "Card",
                    "fields": [
                        { "type": "media", "key": "pic", "label": "Pic", "media_kind": "image" },
                        { "type": "link", "key": "btn", "label": "Btn" }
                    ]
                },
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
        let file_id = sample_hex_id('c');
        let card_pic_id = sample_hex_id('d');

        // Previous values lack `new_required` and item[0].newly_required_sub -> upgrade exemption passes!
        // Unchanged `legacy_unknown_field` from pre-upgrade block data is preserved safely.
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
        cur.insert("manual".into(), json!(file_id));
        cur.insert("body".into(), json!("<p>Xin chào</p>"));
        cur.insert("pat".into(), json!("77"));
        cur.insert("part".into(), json!("hero-header"));
        cur.insert("form".into(), json!("3"));
        cur.insert(
            "pad".into(),
            json!({ "desktop": 40, "tablet": 24, "mobile": 16 }),
        );
        cur.insert(
            "card".into(),
            json!({
                "pic": card_pic_id,
                "btn": { "url": "/xem", "label": "Đọc thêm", "new_tab": false, "nofollow": false }
            }),
        );

        schema.validate_values(&cur, Some(&prev)).unwrap();

        // Injecting or mutating an unknown field fails
        let mut mutated_unknown = cur.clone();
        mutated_unknown.insert("legacy_unknown_field".into(), json!("tampered"));
        assert!(
            schema
                .validate_values(&mutated_unknown, Some(&prev))
                .is_err()
        );

        // Without previous_values (fresh element), missing `new_required` MUST fail
        let mut cur_no_legacy = cur.clone();
        cur_no_legacy.remove("legacy_unknown_field");
        assert!(schema.validate_values(&cur_no_legacy, None).is_err());

        // With show_extra = true and extra_title missing, fail
        let mut show_on = cur.clone();
        show_on.insert("show_extra".into(), json!(true));
        assert!(schema.validate_values(&show_on, Some(&prev)).is_err());

        // Bad responsive key fails
        let mut bad_bp = cur.clone();
        bad_bp.insert("pad".into(), json!({ "wide": 10 }));
        assert!(schema.validate_values(&bad_bp, Some(&prev)).is_err());

        // Media, options_source, & visible text extraction
        let media = schema.collect_media_ids(&cur);
        assert_eq!(media.image_ids, BTreeSet::from([img_id, card_pic_id]));
        assert_eq!(media.any_ids, BTreeSet::from([any_id]));
        assert_eq!(media.file_ids, BTreeSet::from([file_id]));

        assert_eq!(schema.collect_pattern_ids(&cur), BTreeSet::from([77]));
        assert_eq!(
            schema.collect_template_part_keys(&cur),
            vec!["hero-header".to_owned()]
        );
        assert!(schema.has_active_contact_form(&cur));

        let mut texts = Vec::new();
        schema.extract_visible_text(&cur, &mut texts);
        assert_eq!(texts, vec!["Xin chào", "Đọc thêm", "Item 1", "Chi tiết 1"]);
    }

    #[test]
    fn validates_negative_margin_responsive_required_and_no_legacy_suffix_collisions() {
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
                    "required": true,
                    "options": [
                        { "value": "left", "label": "Trái" },
                        { "value": "center", "label": "Giữa" },
                        { "value": "right", "label": "Phải" }
                    ],
                    "default": "center",
                    "responsive": true,
                    "css_var": "--od-cta-align"
                },
                {
                    "type": "spacing",
                    "key": "outer_margin",
                    "label": "Margin",
                    "property": "margin",
                    "min": -128,
                    "max": 128,
                    "unit": "px",
                    "responsive": true
                },
                {
                    "type": "link",
                    "key": "cta",
                    "label": "Nút chính",
                    "required": true,
                    "default": { "url": "/lien-he", "label": "Liên hệ", "new_tab": false, "nofollow": false }
                },
                { "type": "text", "key": "cta_target", "label": "Formerly collided key now allowed in V1" },
                { "type": "text", "key": "outer_margin_classes", "label": "Also allowed in V1" }
            ]
        }))
        .unwrap();
        schema.validate("my-theme").unwrap();

        let valid_vals = BTreeMap::from([
            (
                "align".into(),
                json!({ "desktop": "center", "mobile": null }),
            ),
            (
                "outer_margin".into(),
                json!({ "desktop": { "top": -32, "right": 0, "bottom": 24, "left": 0 } }),
            ),
            (
                "cta".into(),
                json!({ "url": "https://opendash.vn", "label": "Go", "new_tab": true, "nofollow": true }),
            ),
        ]);
        schema.validate_values(&valid_vals, None).unwrap();

        let mut all_null_align = valid_vals.clone();
        all_null_align.insert("align".into(), json!({ "desktop": null, "mobile": "" }));
        assert!(schema.validate_values(&all_null_align, None).is_err());

        let mut bad_spacing = valid_vals.clone();
        bad_spacing.insert("outer_margin".into(), json!({ "desktop": { "top": -200 } }));
        assert!(schema.validate_values(&bad_spacing, None).is_err());

        let mut bad_link = valid_vals;
        bad_link.insert(
            "cta".into(),
            json!({ "url": "javascript:alert(1)", "label": "Bad", "new_tab": false, "nofollow": false }),
        );
        assert!(schema.validate_values(&bad_link, None).is_err());
    }
}
