use super::*;

/// Validates an `EditorLayoutConfigV1` against `Rule LAY-1`..`Rule LAY-6` and `Rule COMPAT-4`,
/// auto-appending any newly registered panels into their `default_region` and returning
/// the deterministically sorted layout alongside the complete `PanelDefinition` list.
pub fn resolve_and_validate_editor_layout(
    effective: &EffectiveContentSchema,
    base_panels: Vec<PanelDefinition>,
    raw_layout: Option<EditorLayoutConfigV1>,
    layout_version: u64,
) -> Result<(EditorLayoutConfigV1, Vec<PanelDefinition>), LayoutValidationError> {
    let type_key = &effective.base.descriptor.key;

    let mut layout = raw_layout.unwrap_or(EditorLayoutConfigV1 {
        schema_version: 1,
        type_key: type_key.clone(),
        version: layout_version,
        regions: EditorRegionsConfig::default(),
    });

    if layout.schema_version != 1 {
        return Err(LayoutValidationError::new(
            "unsupported_layout_schema_version",
            "schema_version của EditorLayoutConfigV1 phải bằng 1.",
        ));
    }
    if layout.type_key != *type_key {
        return Err(LayoutValidationError::new(
            "mismatched_type_key",
            "type_key trong EditorLayoutConfigV1 không khớp với đường dẫn URL.",
        ));
    }

    let total_placements = layout.regions.main_top.len()
        + layout.regions.main_primary.len()
        + layout.regions.main_bottom.len()
        + layout.regions.sidebar.len();
    if total_placements > 64 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            "Tổng số panel placement trên 4 vùng vượt quá giới hạn 64.",
        ));
    }

    let all_panels = base_panels;
    let panels_by_id: HashMap<String, PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.clone(), p.clone()))
        .collect();

    let group_by_id: HashMap<&str, &AttachedFieldGroupSnapshot> = effective
        .attached_groups
        .iter()
        .map(|g| (g.group_id.as_str(), g))
        .collect();

    let mut panel_fields: HashMap<String, Vec<String>> = HashMap::new();
    for g in &effective.attached_groups {
        panel_fields.insert(
            g.group_id.clone(),
            g.fields.iter().map(|f| f.key.clone()).collect(),
        );
    }

    let mut placed_ids: HashSet<String> = HashSet::new();
    let region_pairs = [
        (EditorRegion::MainTop, &mut layout.regions.main_top),
        (EditorRegion::MainPrimary, &mut layout.regions.main_primary),
        (EditorRegion::MainBottom, &mut layout.regions.main_bottom),
        (EditorRegion::Sidebar, &mut layout.regions.sidebar),
    ];

    for (region, placements) in region_pairs {
        for p in placements.iter() {
            let Some(def) = panels_by_id.get(&p.panel_id) else {
                return Err(LayoutValidationError::new(
                    "unknown_or_inapplicable_panel_id",
                    format!(
                        "panel_id '{}' không tồn tại hoặc không áp dụng cho '{type_key}'.",
                        p.panel_id
                    ),
                ));
            };
            if !placed_ids.insert(p.panel_id.clone()) {
                return Err(LayoutValidationError::new(
                    "duplicate_panel_placement",
                    format!(
                        "panel_id '{}' chỉ được xuất hiện tối đa 1 lần trong regions.",
                        p.panel_id
                    ),
                ));
            }
            if !def.allowed_regions.contains(&region) {
                return Err(LayoutValidationError::new(
                    "invalid_panel_region",
                    format!(
                        "Panel '{}' không được phép đặt vào vùng '{}'.",
                        p.panel_id,
                        region.as_str()
                    ),
                ));
            }
            if def.required_core && !p.enabled {
                return Err(LayoutValidationError::new(
                    "required_core_panel_disabled",
                    format!("Panel lõi '{}' không thể bị vô hiệu hóa.", p.panel_id),
                ));
            }
            if !p.subgroups.is_empty() {
                if !matches!(def.kind, PanelKind::ContentFields { .. }) {
                    return Err(LayoutValidationError::new(
                        "subgroups_not_allowed_on_panel_kind",
                        format!("Panel '{}' không hỗ trợ subgroups.", p.panel_id),
                    ));
                }
                let fkeys = panel_fields
                    .get(&p.panel_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                validate_subgroups_for_panel(&p.subgroups, fkeys, &p.panel_id)?;
            }

            let check_required_panel = group_by_id.get(p.panel_id.as_str()).is_none_or(|g| {
                g.active
                    && group_allows_role_for_target(
                        &g.location,
                        "content_type",
                        type_key,
                        FieldRole::Editor,
                    )
            });
            if check_required_panel && let Some(fkeys) = panel_fields.get(&p.panel_id) {
                for fk in fkeys {
                    if let Some(eff) = effective.effective_fields.get(fk)
                        && eff.effective_required()
                        && eff.definition.effective_acl().1 == FieldRole::Editor
                        && (!p.enabled
                            || def.min_view_role == FieldRole::Admin
                            || def.min_edit_role == FieldRole::Admin)
                    {
                        return Err(LayoutValidationError::new(
                            "required_field_hidden_or_inaccessible",
                            format!(
                                "Không thể ẩn hoặc giới hạn quyền Admin cho panel '{}' vì chứa trường bắt buộc '{fk}'.",
                                p.panel_id
                            ),
                        ));
                    }
                }
            }
        }
    }

    for def in &all_panels {
        if def.default_region == EditorRegion::HeaderBar || placed_ids.contains(&def.panel_id) {
            continue;
        }
        let default_placement = PanelPlacementConfig {
            panel_id: def.panel_id.clone(),
            enabled: true,
            order: def.default_order,
            collapsed_by_default: def.presentation.collapsed_by_default,
            subgroups: Vec::new(),
        };
        match def.default_region {
            EditorRegion::MainTop => layout.regions.main_top.push(default_placement),
            EditorRegion::MainPrimary => layout.regions.main_primary.push(default_placement),
            EditorRegion::MainBottom => layout.regions.main_bottom.push(default_placement),
            EditorRegion::Sidebar => layout.regions.sidebar.push(default_placement),
            EditorRegion::HeaderBar => {}
        }
        placed_ids.insert(def.panel_id.clone());
    }

    let sort_region = |list: &mut Vec<PanelPlacementConfig>| {
        list.sort_by(|a, b| {
            let ka = panels_by_id
                .get(&a.panel_id)
                .map(|d| panel_sort_key(d, a))
                .unwrap_or((a.order, 99, a.panel_id.clone()));
            let kb = panels_by_id
                .get(&b.panel_id)
                .map(|d| panel_sort_key(d, b))
                .unwrap_or((b.order, 99, b.panel_id.clone()));
            ka.cmp(&kb)
        });
    };
    sort_region(&mut layout.regions.main_top);
    sort_region(&mut layout.regions.main_primary);
    sort_region(&mut layout.regions.main_bottom);
    sort_region(&mut layout.regions.sidebar);

    layout.version = layout_version;
    Ok((layout, all_panels))
}

/// Projects `EffectiveEditorConfigProjection` for `caller_role` (`Editor` | `Admin`),
/// hiding fields/panels where `caller_role < min_view_role`, inactive groups, or
/// admin-only location rules, and marking `editable: false` where `caller_role < min_edit_role`.
pub fn project_effective_editor_config(
    effective: &EffectiveContentSchema,
    layout: &EditorLayoutConfigV1,
    all_panels: &[PanelDefinition],
    taxonomies: &[String],
    caller_role: FieldRole,
) -> EffectiveEditorConfigProjection {
    let type_key = &effective.base.descriptor.key;
    let panels_by_id: HashMap<&str, &PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.as_str(), p))
        .collect();
    let group_by_id: HashMap<&str, &AttachedFieldGroupSnapshot> = effective
        .attached_groups
        .iter()
        .map(|g| (g.group_id.as_str(), g))
        .collect();

    let mut projected_fields = BTreeMap::new();
    for (k, entry) in &effective.effective_fields {
        if caller_role < entry.definition.effective_acl().0 {
            continue;
        }
        if entry.owner != ResourceOwner::Core
            && let Some(group) = group_by_id.get(entry.source_id.as_str())
        {
            if !group.active {
                continue;
            }
            if !group_allows_role_for_target(&group.location, "content_type", type_key, caller_role)
            {
                continue;
            }
        }
        projected_fields.insert(
            k.clone(),
            EffectiveFieldProjection {
                definition: entry.definition.clone(),
                owner: entry.owner.clone(),
                source_id: entry.source_id.clone(),
                effective_status: entry.effective_status,
                effective_required: entry.effective_required(),
                editable: caller_role >= entry.definition.effective_acl().1,
            },
        );
    }

    let group_specs_map: HashMap<&str, &[crate::field_schema::FieldSpec]> = effective
        .attached_groups
        .iter()
        .map(|g| (g.group_id.as_str(), g.fields.as_slice()))
        .collect();
    let group_fields_map: HashMap<&str, Vec<String>> = group_specs_map
        .iter()
        .map(|(&gid, specs)| (gid, specs.iter().map(|f| f.key.clone()).collect()))
        .collect();

    let project_region = |placements: &[PanelPlacementConfig]| -> Vec<EffectivePanelProjection> {
        let mut out = Vec::new();
        for p in placements {
            if !p.enabled {
                continue;
            }
            let Some(def) = panels_by_id.get(p.panel_id.as_str()).copied() else {
                continue;
            };
            if caller_role < def.min_view_role {
                continue;
            }
            let (raw_keys, group_meta) = match &def.kind {
                PanelKind::ContentFields { field_group_id } => {
                    let canonical_gid = field_group_id.to_canonical();
                    let Some(group) = group_by_id.get(canonical_gid.as_str()).copied() else {
                        continue;
                    };
                    if !group.active
                        || !group_allows_role_for_target(
                            &group.location,
                            "content_type",
                            type_key,
                            caller_role,
                        )
                    {
                        continue;
                    }
                    let keys = group_fields_map
                        .get(canonical_gid.as_str())
                        .cloned()
                        .unwrap_or_default();
                    (keys, Some(group))
                }
                _ => (Vec::new(), None),
            };
            let visible_keys: Vec<String> = raw_keys
                .into_iter()
                .filter(|fk| projected_fields.contains_key(fk))
                .collect();
            if matches!(def.kind, PanelKind::ContentFields { .. }) && visible_keys.is_empty() {
                continue;
            }
            let visible_set: HashSet<&str> = visible_keys.iter().map(String::as_str).collect();
            let panel_fields = panel_fields::collect_panel_fields(
                def,
                &group_specs_map,
                &projected_fields,
                &visible_set,
            );
            let subgroups = panel_fields::collect_panel_subgroups(p, &visible_set);

            out.push(EffectivePanelProjection {
                panel: def.clone(),
                placement: p.clone(),
                field_keys: visible_keys,
                subgroups,
                panel_fields,
                location: group_meta.map(|g| g.location.clone()),
                active: group_meta.is_none_or(|g| g.active),
            });
        }
        out
    };

    EffectiveEditorConfigProjection {
        type_key: effective.base.descriptor.key.clone(),
        label: effective.base.descriptor.label.clone(),
        singular_label: effective.base.descriptor.singular_label.clone(),
        description: effective.base.descriptor.description.clone(),
        settings: effective.base.descriptor.settings.clone(),
        owner: effective.base.owner.clone(),
        provider_status: effective.base.status,
        declaration_state: effective.base.declaration_state,
        base_version: effective.base_version,
        layout_token: format!("v{}", layout.version),
        schema_token: effective.schema_token.clone(),
        taxonomies: taxonomies.to_vec(),
        regions: EffectiveEditorRegionsProjection {
            main_top: project_region(&layout.regions.main_top),
            main_primary: project_region(&layout.regions.main_primary),
            main_bottom: project_region(&layout.regions.main_bottom),
            sidebar: project_region(&layout.regions.sidebar),
        },
        effective_fields: projected_fields,
    }
}
