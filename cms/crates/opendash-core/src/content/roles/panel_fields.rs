use super::*;
use crate::field_schema::FieldSpec;

/// Thu thập danh sách `FieldSpec` theo thứ tự khai báo cho một panel đang được chiếu
/// (giữ nguyên các phần tử chỉ-UI như `tab`/`accordion`/`message` cùng thứ tự gốc).
pub(super) fn collect_panel_fields(
    def: &PanelDefinition,
    effective: &EffectiveContentSchema,
    group_specs_map: &HashMap<&str, &[FieldSpec]>,
    assigned_base: &HashSet<&str>,
    projected_fields: &BTreeMap<String, EffectiveFieldProjection>,
    visible_keys: &[String],
    visible_set: &HashSet<&str>,
) -> Vec<FieldSpec> {
    let mut panel_fields = Vec::new();
    match &def.kind {
        PanelKind::ContentFields { field_group_id } => {
            if let Some(specs) = group_specs_map.get(field_group_id.to_canonical().as_str()) {
                for spec in *specs {
                    if spec.is_ui_only() {
                        panel_fields.push(spec.clone());
                    } else if visible_set.contains(spec.key.as_str())
                        && let Some(proj) = projected_fields.get(&spec.key)
                    {
                        panel_fields.push(proj.definition.clone());
                    }
                }
            }
        }
        PanelKind::CoreBuiltIn { core_key } if core_key == "base_fields" => {
            for spec in &effective.base.descriptor.fields {
                if (spec.is_ui_only() && assigned_base.is_empty())
                    || visible_set.contains(spec.key.as_str())
                {
                    if spec.is_ui_only() {
                        panel_fields.push(spec.clone());
                    } else if let Some(proj) = projected_fields.get(&spec.key) {
                        panel_fields.push(proj.definition.clone());
                    }
                }
            }
        }
        PanelKind::BaseFieldsSlice { .. } => {
            for fk in visible_keys {
                if let Some(proj) = projected_fields.get(fk) {
                    panel_fields.push(proj.definition.clone());
                }
            }
        }
        _ => {}
    }
    panel_fields
}

/// Lọc các subgroup trường hiển thị được của panel theo tập khóa `visible_set`.
pub(super) fn collect_panel_subgroups(
    placement: &PanelPlacementConfig,
    base_field_panels: &[BaseFieldSlicePanelConfig],
    visible_set: &HashSet<&str>,
) -> Vec<LayoutFieldSubgroup> {
    let raw_subgroups = if !placement.subgroups.is_empty() {
        &placement.subgroups
    } else if let Some(slice) = base_field_panels
        .iter()
        .find(|s| s.panel_id == placement.panel_id)
    {
        &slice.subgroups
    } else {
        &[][..]
    };
    raw_subgroups
        .iter()
        .filter_map(|sg| {
            let fks: Vec<String> = sg
                .field_keys
                .iter()
                .filter(|fk| visible_set.contains(fk.as_str()))
                .cloned()
                .collect();
            if fks.is_empty() {
                None
            } else {
                Some(LayoutFieldSubgroup {
                    subgroup_id: sg.subgroup_id.clone(),
                    title: sg.title.clone(),
                    field_keys: fks,
                })
            }
        })
        .collect()
}
