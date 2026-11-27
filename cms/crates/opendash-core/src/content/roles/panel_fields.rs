use super::*;
use crate::field_schema::FieldSpec;

/// Thu thập danh sách `FieldSpec` theo thứ tự khai báo cho một panel đang được chiếu
/// (giữ nguyên các phần tử chỉ-UI như `tab`/`accordion`/`message` cùng thứ tự gốc).
pub(super) fn collect_panel_fields(
    def: &PanelDefinition,
    group_specs_map: &HashMap<&str, &[FieldSpec]>,
    projected_fields: &BTreeMap<String, EffectiveFieldProjection>,
    visible_set: &HashSet<&str>,
) -> Vec<FieldSpec> {
    let mut panel_fields = Vec::new();
    if let PanelKind::ContentFields { field_group_id } = &def.kind
        && let Some(specs) = group_specs_map.get(field_group_id.to_canonical().as_str())
    {
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
    panel_fields
}

/// Lọc các subgroup trường hiển thị được của panel theo tập khóa `visible_set`.
pub(super) fn collect_panel_subgroups(
    placement: &PanelPlacementConfig,
    visible_set: &HashSet<&str>,
) -> Vec<LayoutFieldSubgroup> {
    placement
        .subgroups
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
