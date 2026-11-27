use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]

pub struct PanelDefinition {
    pub panel_id: String,
    pub owner: ResourceOwner,
    pub title: String,
    pub kind: PanelKind,
    pub applies_to: Vec<String>,
    pub default_region: EditorRegion,
    pub allowed_regions: Vec<EditorRegion>,
    pub default_order: i32,
    pub min_view_role: FieldRole,
    pub min_edit_role: FieldRole,
    pub collapsible: bool,
    pub required_core: bool,
    pub effective_status: EffectiveFieldStatus,
}

/// Wire DTO for `editor_panels` (`Action` panels) in `plugin.json` (`manifest_schema: 2`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestActionPanelInput {
    pub local_key: String,
    pub title: String,
    pub action_id: String,
    pub button_label: String,
    #[serde(default)]
    pub confirm_message: Option<String>,
    #[serde(default)]
    pub public_effect: bool,
    pub applies_to: Vec<String>,
    #[serde(default = "EditorRegion::default_sidebar")]
    pub default_region: EditorRegion,
    #[serde(default = "default_action_allowed_regions")]
    pub allowed_regions: Vec<EditorRegion>,
    #[serde(default = "default_panel_order_80")]
    pub default_order: i32,
    #[serde(default)]
    pub min_view_role: Option<FieldRole>,
    #[serde(default)]
    pub min_edit_role: Option<FieldRole>,
    #[serde(default = "default_true")]
    pub collapsible: bool,
}

impl ManifestActionPanelInput {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.local_key.starts_with("core:") || self.local_key.starts_with("layout:") {
            return Err(
                "reserved_panel_namespace: Không được khai báo tiền tố 'core:' hoặc 'layout:' trong editor_panels.",
            );
        }
        if !valid_snake_key(&self.local_key) {
            return Err("local_key của editor_panel phải là snake_case hợp lệ (1..=64 ký tự).");
        }
        if !valid_label(&self.title) || !valid_label(&self.button_label) {
            return Err("Tiêu đề hoặc nhãn nút của editor_panel không hợp lệ.");
        }
        if let Some(msg) = &self.confirm_message {
            if msg.chars().count() > 300 || msg.chars().any(char::is_control) {
                return Err("confirm_message của editor_panel không hợp lệ.");
            }
        }
        if self.applies_to.is_empty() || self.applies_to.len() > 16 {
            return Err("applies_to của editor_panel phải có từ 1 đến 16 loại nội dung.");
        }
        let mut seen_types = HashSet::new();
        for t in &self.applies_to {
            if !valid_content_type_key(t) || !seen_types.insert(t.as_str()) {
                return Err("applies_to của editor_panel không hợp lệ hoặc bị trùng lặp.");
            }
        }
        if self.allowed_regions.is_empty() {
            return Err("invalid_panel_region: allowed_regions không được rỗng.");
        }
        for r in &self.allowed_regions {
            if !matches!(
                r,
                EditorRegion::MainTop | EditorRegion::MainBottom | EditorRegion::Sidebar
            ) {
                return Err(
                    "invalid_panel_region: Action panel chỉ được phép đặt trong main_top, main_bottom hoặc sidebar.",
                );
            }
        }
        if !self.allowed_regions.contains(&self.default_region) {
            return Err("invalid_panel_region: default_region phải nằm trong allowed_regions.");
        }
        if !(-1000..=1000).contains(&self.default_order) {
            return Err("default_order của editor_panel phải nằm trong [-1000..=1000].");
        }
        let min_view = self.min_view_role.unwrap_or(FieldRole::Editor);
        let min_edit = self.min_edit_role.unwrap_or(FieldRole::Editor);
        if min_edit < min_view {
            return Err("invalid_panel_acl: min_edit_role không được thấp hơn min_view_role.");
        }
        Ok(())
    }
}

pub fn build_core_and_group_panels(
    effective: &EffectiveContentSchema,
    has_taxonomies: bool,
    plugin_action_panels: &[PanelDefinition],
) -> Vec<PanelDefinition> {
    let type_key = &effective.base.descriptor.key;
    let use_body_panel = has_core_body_panel(effective);
    let mut panels = Vec::new();
    let all_regions = default_all_content_regions();

    panels.push(PanelDefinition {
        panel_id: "core:header_bar".into(),
        owner: ResourceOwner::Core,
        title: "Thanh tiêu đề".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "header_bar".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::HeaderBar,
        allowed_regions: vec![EditorRegion::HeaderBar],
        default_order: 0,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:title_slug".into(),
        owner: ResourceOwner::Core,
        title: "Tiêu đề & Đường dẫn".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "title_slug".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainPrimary,
        allowed_regions: all_regions.clone(),
        default_order: 10,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    if use_body_panel {
        panels.push(PanelDefinition {
            panel_id: "core:body_editor".into(),
            owner: ResourceOwner::Core,
            title: "Nội dung chính".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "body_editor".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::MainPrimary,
            allowed_regions: all_regions.clone(),
            default_order: 20,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: false,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    panels.push(PanelDefinition {
        panel_id: "core:base_fields".into(),
        owner: ResourceOwner::Core,
        title: "Trường cơ bản".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "base_fields".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainPrimary,
        allowed_regions: all_regions.clone(),
        default_order: 30,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: true,
        required_core: false,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:seo".into(),
        owner: ResourceOwner::Core,
        title: "Cấu hình SEO".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "seo".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::MainBottom,
        allowed_regions: all_regions.clone(),
        default_order: 90,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: true,
        required_core: false,
        effective_status: EffectiveFieldStatus::Active,
    });

    panels.push(PanelDefinition {
        panel_id: "core:publish_box".into(),
        owner: ResourceOwner::Core,
        title: "Xuất bản & Trạng thái".into(),
        kind: PanelKind::CoreBuiltIn {
            core_key: "publish_box".into(),
        },
        applies_to: vec![type_key.clone()],
        default_region: EditorRegion::Sidebar,
        allowed_regions: all_regions.clone(),
        default_order: 10,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible: false,
        required_core: true,
        effective_status: EffectiveFieldStatus::Active,
    });

    if type_key == "page" {
        panels.push(PanelDefinition {
            panel_id: "core:page_attributes".into(),
            owner: ResourceOwner::Core,
            title: "Thuộc tính trang".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "page_attributes".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 20,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    if !matches!(type_key.as_str(), "section" | "pattern")
        && effective.effective_fields.contains_key("featured_image")
    {
        panels.push(PanelDefinition {
            panel_id: "core:featured_image".into(),
            owner: ResourceOwner::Core,
            title: "Ảnh đại diện".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "featured_image".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 30,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    if type_key == "post" || has_taxonomies {
        panels.push(PanelDefinition {
            panel_id: "core:taxonomies".into(),
            owner: ResourceOwner::Core,
            title: "Phân loại".into(),
            kind: PanelKind::CoreBuiltIn {
                core_key: "taxonomies".into(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::Sidebar,
            allowed_regions: all_regions.clone(),
            default_order: 40,
            min_view_role: FieldRole::Editor,
            min_edit_role: FieldRole::Editor,
            collapsible: true,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    for group in &effective.attached_groups {
        let Ok(fg_id) = ScopedResourceId::parse(&group.group_id) else {
            continue;
        };
        let min_view_role = group
            .fields
            .iter()
            .map(|f| f.effective_acl().0)
            .min()
            .unwrap_or(FieldRole::Editor);
        let min_edit_role = group
            .fields
            .iter()
            .map(|f| f.effective_acl().1)
            .min()
            .unwrap_or(FieldRole::Editor);
        let effective_status = if group.status == FieldGroupStatus::Withdrawn
            || group.association_state == DeclarationState::Withdrawn
        {
            EffectiveFieldStatus::Withdrawn
        } else if group.status == FieldGroupStatus::ProviderInactive {
            EffectiveFieldStatus::ProviderInactive
        } else {
            EffectiveFieldStatus::Active
        };
        panels.push(PanelDefinition {
            panel_id: group.group_id.clone(),
            owner: group.owner.clone(),
            title: group.title.clone(),
            kind: PanelKind::ContentFields {
                field_group_id: fg_id,
            },
            applies_to: vec![type_key.clone()],
            default_region: group.default_region,
            allowed_regions: all_regions.clone(),
            default_order: group.default_order,
            min_view_role,
            min_edit_role,
            collapsible: group.collapsible,
            required_core: false,
            effective_status,
        });
    }

    for ap in plugin_action_panels {
        if ap.applies_to.iter().any(|t| t == type_key) {
            panels.push(ap.clone());
        }
    }

    panels
}

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
    let eligible_base = eligible_base_field_keys(effective);
    let eligible_set: HashSet<&str> = eligible_base.iter().map(String::as_str).collect();

    let mut layout = raw_layout.unwrap_or(EditorLayoutConfigV1 {
        schema_version: 1,
        type_key: type_key.clone(),
        version: layout_version,
        base_field_panels: Vec::new(),
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
    if layout.base_field_panels.len() > 16 {
        return Err(LayoutValidationError::new(
            "layout_quota_exceeded",
            "Mỗi loại nội dung hỗ trợ tối đa 16 base_field_panels.",
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

    // Rule LAY-2: Build BaseFieldsSlice PanelDefinitions & check base_field_panels partitioning
    let mut all_panels = base_panels;
    let mut seen_slice_ids = HashSet::new();
    let mut assigned_base_fields = HashSet::new();
    let mut slice_field_map: HashMap<String, Vec<String>> = HashMap::new();

    for slice in &layout.base_field_panels {
        let Some(slice_key) = crate::id::valid_layout_slice_panel_id(&slice.panel_id) else {
            return Err(LayoutValidationError::new(
                "invalid_or_duplicate_slice_panel_id",
                format!(
                    "panel_id '{}' phải có dạng 'layout:<snake_key>'.",
                    slice.panel_id
                ),
            ));
        };
        if !seen_slice_ids.insert(slice.panel_id.as_str()) {
            return Err(LayoutValidationError::new(
                "invalid_or_duplicate_slice_panel_id",
                format!("Trùng lặp base_field_panels panel_id '{}'.", slice.panel_id),
            ));
        }
        if !valid_label(&slice.title) {
            return Err(LayoutValidationError::new(
                "invalid_panel_title",
                format!("Tiêu đề panel '{}' không hợp lệ.", slice.panel_id),
            ));
        }
        if slice.field_keys.is_empty() || slice.field_keys.len() > 64 {
            return Err(LayoutValidationError::new(
                "layout_quota_exceeded",
                format!(
                    "Panel '{}' phải chứa từ 1 đến 64 field_keys.",
                    slice.panel_id
                ),
            ));
        }
        let mut min_view = FieldRole::Admin;
        let mut min_edit = FieldRole::Admin;
        for fk in &slice.field_keys {
            if !eligible_set.contains(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "unknown_base_field_key",
                    format!(
                        "Trường '{fk}' không thuộc danh sách base fields hợp lệ của '{type_key}'."
                    ),
                ));
            }
            if !assigned_base_fields.insert(fk.as_str()) {
                return Err(LayoutValidationError::new(
                    "duplicate_base_field_placement",
                    format!(
                        "Base field '{fk}' chỉ được xuất hiện tối đa 1 lần trong base_field_panels."
                    ),
                ));
            }
            if let Some(eff) = effective.effective_fields.get(fk) {
                if eff.definition.effective_acl().0 < min_view {
                    min_view = eff.definition.effective_acl().0;
                }
                if eff.definition.effective_acl().1 < min_edit {
                    min_edit = eff.definition.effective_acl().1;
                }
            }
        }
        validate_subgroups_for_panel(&slice.subgroups, &slice.field_keys, &slice.panel_id)?;
        slice_field_map.insert(slice.panel_id.clone(), slice.field_keys.clone());
        all_panels.push(PanelDefinition {
            panel_id: slice.panel_id.clone(),
            owner: ResourceOwner::User,
            title: slice.title.clone(),
            kind: PanelKind::BaseFieldsSlice {
                slice_key: slice_key.to_owned(),
                field_keys: slice.field_keys.clone(),
            },
            applies_to: vec![type_key.clone()],
            default_region: EditorRegion::MainBottom,
            allowed_regions: default_all_content_regions(),
            default_order: 50,
            min_view_role: min_view,
            min_edit_role: min_edit,
            collapsible: slice.collapsible,
            required_core: false,
            effective_status: EffectiveFieldStatus::Active,
        });
    }

    let unassigned_base_fields: Vec<String> = eligible_base
        .into_iter()
        .filter(|fk| !assigned_base_fields.contains(fk.as_str()))
        .collect();

    let panels_by_id: HashMap<String, PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.clone(), p.clone()))
        .collect();

    // Map each panel_id -> list of field keys belonging to it (for Rule COMPAT-4 and Rule LAY-5)
    let mut panel_fields: HashMap<String, Vec<String>> = slice_field_map;
    panel_fields.insert("core:base_fields".into(), unassigned_base_fields.clone());
    for g in &effective.attached_groups {
        panel_fields.insert(
            g.group_id.clone(),
            g.fields.iter().map(|f| f.key.clone()).collect(),
        );
    }

    // Rule LAY-3, LAY-4, LAY-5: Validate every placement in regions
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
                let allows_subgroups = matches!(
                    def.kind,
                    PanelKind::ContentFields { .. } | PanelKind::BaseFieldsSlice { .. }
                ) || p.panel_id == "core:base_fields";
                if !allows_subgroups {
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
            // Rule COMPAT-4: panel containing effective_required == true && min_edit_role == Editor cannot be hidden/admin-only
            if let Some(fkeys) = panel_fields.get(&p.panel_id) {
                for fk in fkeys {
                    if let Some(eff) = effective.effective_fields.get(fk) {
                        if eff.effective_required()
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
    }

    // Rule LAY-2: Every slice in base_field_panels must be placed in regions
    for slice in &layout.base_field_panels {
        if !placed_ids.contains(&slice.panel_id) {
            return Err(LayoutValidationError::new(
                "unplaced_base_field_slice",
                format!(
                    "Panel '{}' trong base_field_panels chưa được đặt vào vùng nào.",
                    slice.panel_id
                ),
            ));
        }
    }

    // Rule LAY-6: Auto-append any newly registered panels missing from saved regions
    for def in &all_panels {
        if def.default_region == EditorRegion::HeaderBar || placed_ids.contains(&def.panel_id) {
            continue;
        }
        let default_placement = PanelPlacementConfig {
            panel_id: def.panel_id.clone(),
            enabled: true,
            order: def.default_order,
            collapsed_by_default: false,
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

    // Deterministic sorting in each region
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
/// hiding fields/panels where `caller_role < min_view_role` and marking `editable: false`
/// where `caller_role < min_edit_role`.
pub fn project_effective_editor_config(
    effective: &EffectiveContentSchema,
    layout: &EditorLayoutConfigV1,
    all_panels: &[PanelDefinition],
    taxonomies: &[String],
    caller_role: FieldRole,
) -> EffectiveEditorConfigProjection {
    let panels_by_id: HashMap<&str, &PanelDefinition> = all_panels
        .iter()
        .map(|p| (p.panel_id.as_str(), p))
        .collect();

    let mut projected_fields = BTreeMap::new();
    for (k, entry) in &effective.effective_fields {
        if caller_role < entry.definition.effective_acl().0 {
            continue;
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

    let eligible_base = eligible_base_field_keys(effective);
    let assigned_base: HashSet<&str> = layout
        .base_field_panels
        .iter()
        .flat_map(|s| s.field_keys.iter().map(String::as_str))
        .collect();
    let unassigned_base: Vec<String> = eligible_base
        .into_iter()
        .filter(|fk| !assigned_base.contains(fk.as_str()))
        .collect();

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
            let raw_keys: Vec<String> = match &def.kind {
                PanelKind::BaseFieldsSlice { field_keys, .. } => field_keys.clone(),
                PanelKind::CoreBuiltIn { core_key } if core_key == "base_fields" => {
                    if unassigned_base.is_empty() {
                        continue;
                    }
                    unassigned_base.clone()
                }
                PanelKind::ContentFields { field_group_id } => group_fields_map
                    .get(field_group_id.to_canonical().as_str())
                    .cloned()
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            let visible_keys: Vec<String> = raw_keys
                .into_iter()
                .filter(|fk| projected_fields.contains_key(fk))
                .collect();
            if matches!(
                def.kind,
                PanelKind::BaseFieldsSlice { .. } | PanelKind::ContentFields { .. }
            ) && visible_keys.is_empty()
            {
                continue;
            }
            if p.panel_id == "core:base_fields" && visible_keys.is_empty() {
                continue;
            }
            let visible_set: HashSet<&str> = visible_keys.iter().map(String::as_str).collect();
            let panel_fields = panel_fields::collect_panel_fields(
                def,
                effective,
                &group_specs_map,
                &assigned_base,
                &projected_fields,
                &visible_keys,
                &visible_set,
            );
            let subgroups =
                panel_fields::collect_panel_subgroups(p, &layout.base_field_panels, &visible_set);

            out.push(EffectivePanelProjection {
                panel: def.clone(),
                placement: p.clone(),
                field_keys: visible_keys,
                subgroups,
                panel_fields,
            });
        }
        out
    };

    EffectiveEditorConfigProjection {
        type_key: effective.base.descriptor.key.clone(),
        label: effective.base.descriptor.label.clone(),
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
