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
    #[serde(default)]
    pub presentation: FieldGroupPresentation,
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
        if let Some(msg) = &self.confirm_message
            && (msg.chars().count() > 300 || msg.chars().any(char::is_control))
        {
            return Err("confirm_message của editor_panel không hợp lệ.");
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

fn core_panel(
    panel_id: &str,
    core_key: &str,
    title: &str,
    type_key: &str,
    default_region: EditorRegion,
    allowed_regions: Vec<EditorRegion>,
    default_order: i32,
    collapsible: bool,
    required_core: bool,
) -> PanelDefinition {
    PanelDefinition {
        panel_id: panel_id.to_owned(),
        owner: ResourceOwner::Core,
        title: title.to_owned(),
        kind: PanelKind::CoreBuiltIn {
            core_key: core_key.to_owned(),
        },
        applies_to: vec![type_key.to_owned()],
        default_region,
        allowed_regions,
        default_order,
        min_view_role: FieldRole::Editor,
        min_edit_role: FieldRole::Editor,
        collapsible,
        presentation: FieldGroupPresentation {
            region: default_region,
            order: default_order,
            collapsible,
            ..FieldGroupPresentation::default()
        },
        required_core,
        effective_status: EffectiveFieldStatus::Active,
    }
}

pub fn build_core_and_group_panels(
    effective: &EffectiveContentSchema,
    has_taxonomies: bool,
    plugin_action_panels: &[PanelDefinition],
) -> Vec<PanelDefinition> {
    let type_key = &effective.base.descriptor.key;
    let settings = &effective.base.descriptor.settings;
    let use_body_panel = has_core_body_panel(effective);
    let mut panels = Vec::new();
    let all_regions = default_all_content_regions();

    panels.push(core_panel(
        "core:header_bar",
        "header_bar",
        "Thanh tiêu đề",
        type_key,
        EditorRegion::HeaderBar,
        vec![EditorRegion::HeaderBar],
        0,
        false,
        true,
    ));
    panels.push(core_panel(
        "core:title_slug",
        "title_slug",
        "Tiêu đề & Đường dẫn",
        type_key,
        EditorRegion::MainPrimary,
        all_regions.clone(),
        10,
        false,
        true,
    ));

    if use_body_panel {
        panels.push(core_panel(
            "core:body_editor",
            "body_editor",
            "Nội dung chính",
            type_key,
            EditorRegion::MainPrimary,
            all_regions.clone(),
            20,
            false,
            false,
        ));
    }

    if !matches!(type_key.as_str(), "section" | "pattern")
        && settings.supports(ContentSupport::Excerpt)
    {
        panels.push(core_panel(
            "core:excerpt",
            "excerpt",
            "Tóm tắt",
            type_key,
            EditorRegion::MainBottom,
            all_regions.clone(),
            25,
            true,
            false,
        ));
    }

    panels.push(core_panel(
        "core:seo",
        "seo",
        "Cấu hình SEO",
        type_key,
        EditorRegion::MainBottom,
        all_regions.clone(),
        90,
        true,
        false,
    ));

    panels.push(core_panel(
        "core:publish_box",
        "publish_box",
        "Xuất bản & Trạng thái",
        type_key,
        EditorRegion::Sidebar,
        all_regions.clone(),
        10,
        false,
        true,
    ));

    if !matches!(type_key.as_str(), "section" | "pattern")
        && (type_key == "page"
            || settings.supports(ContentSupport::CustomTemplate)
            || settings.supports(ContentSupport::MenuOrder)
            || settings.hierarchical)
    {
        panels.push(core_panel(
            "core:page_attributes",
            "page_attributes",
            "Thuộc tính trang",
            type_key,
            EditorRegion::Sidebar,
            all_regions.clone(),
            60,
            true,
            false,
        ));
    }

    if !matches!(type_key.as_str(), "section" | "pattern")
        && effective.effective_fields.contains_key("featured_image")
    {
        panels.push(core_panel(
            "core:featured_image",
            "featured_image",
            "Ảnh đại diện",
            type_key,
            EditorRegion::Sidebar,
            all_regions.clone(),
            30,
            true,
            false,
        ));
    }

    if type_key == "post" || has_taxonomies {
        panels.push(core_panel(
            "core:taxonomies",
            "taxonomies",
            "Phân loại",
            type_key,
            EditorRegion::Sidebar,
            all_regions.clone(),
            40,
            true,
            false,
        ));
    }

    for group in &effective.attached_groups {
        let Ok(fg_id) = ScopedResourceId::parse(&group.group_id) else {
            continue;
        };
        let admin_only = !group_allows_role_for_target(
            &group.location,
            "content_type",
            type_key,
            FieldRole::Editor,
        );
        let min_view_role = if admin_only {
            FieldRole::Admin
        } else {
            group
                .fields
                .iter()
                .filter(|f| !f.is_ui_only())
                .map(|f| f.effective_acl().0)
                .min()
                .unwrap_or(FieldRole::Editor)
        };
        let min_edit_role = if admin_only {
            FieldRole::Admin
        } else {
            group
                .fields
                .iter()
                .filter(|f| !f.is_ui_only())
                .map(|f| f.effective_acl().1)
                .min()
                .unwrap_or(FieldRole::Editor)
        };
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
            default_region: group.presentation.region,
            allowed_regions: all_regions.clone(),
            default_order: group.presentation.order,
            min_view_role,
            min_edit_role,
            collapsible: group.presentation.collapsible,
            presentation: group.presentation,
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
