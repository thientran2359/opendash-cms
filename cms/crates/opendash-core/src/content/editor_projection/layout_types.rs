use super::*;

pub(crate) const fn default_panel_order_80() -> i32 {
    80
}

pub(crate) fn default_action_allowed_regions() -> Vec<EditorRegion> {
    vec![
        EditorRegion::MainTop,
        EditorRegion::MainBottom,
        EditorRegion::Sidebar,
    ]
}

pub(crate) fn default_all_content_regions() -> Vec<EditorRegion> {
    vec![
        EditorRegion::MainTop,
        EditorRegion::MainPrimary,
        EditorRegion::MainBottom,
        EditorRegion::Sidebar,
    ]
}

/// Panel kinds in the Single Edit Panel Registry (`270-R9`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelKind {
    CoreBuiltIn {
        core_key: String,
    },
    BaseFieldsSlice {
        slice_key: String,
        field_keys: Vec<String>,
    },
    ContentFields {
        field_group_id: ScopedResourceId,
    },
    Action {
        action_id: String,
        button_label: String,
        confirm_message: Option<String>,
        public_effect: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutFieldSubgroup {
    pub subgroup_id: String,
    pub title: String,
    pub field_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseFieldSlicePanelConfig {
    pub panel_id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub collapsible: bool,
    pub field_keys: Vec<String>,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelPlacementConfig {
    pub panel_id: String,
    pub enabled: bool,
    pub order: i32,
    #[serde(default)]
    pub collapsed_by_default: bool,
    #[serde(default)]
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorRegionsConfig {
    #[serde(default)]
    pub main_top: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub main_primary: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub main_bottom: Vec<PanelPlacementConfig>,
    #[serde(default)]
    pub sidebar: Vec<PanelPlacementConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditorLayoutConfigV1 {
    pub schema_version: u32,
    pub type_key: String,
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub base_field_panels: Vec<BaseFieldSlicePanelConfig>,
    pub regions: EditorRegionsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveFieldProjection {
    pub definition: ContentFieldDefinition,
    pub owner: ResourceOwner,
    pub source_id: String,
    pub effective_status: EffectiveFieldStatus,
    pub effective_required: bool,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectivePanelProjection {
    pub panel: PanelDefinition,
    pub placement: PanelPlacementConfig,
    pub field_keys: Vec<String>,
    pub subgroups: Vec<LayoutFieldSubgroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEditorRegionsProjection {
    pub main_top: Vec<EffectivePanelProjection>,
    pub main_primary: Vec<EffectivePanelProjection>,
    pub main_bottom: Vec<EffectivePanelProjection>,
    pub sidebar: Vec<EffectivePanelProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveEditorConfigProjection {
    pub type_key: String,
    pub label: String,
    pub settings: ContentTypeSettings,
    pub owner: ResourceOwner,
    pub provider_status: ProviderStatus,
    pub declaration_state: DeclarationState,
    pub base_version: i64,
    pub layout_token: String,
    pub schema_token: String,
    pub taxonomies: Vec<String>,
    pub regions: EffectiveEditorRegionsProjection,
    pub effective_fields: BTreeMap<String, EffectiveFieldProjection>,
}

pub(crate) fn owner_rank(owner: &ResourceOwner) -> u8 {
    match owner {
        ResourceOwner::Core => 0,
        ResourceOwner::User => 2,
        ResourceOwner::Theme(_) => 3,
        ResourceOwner::Plugin(_) => 4,
    }
}

pub(crate) fn panel_sort_key(
    def: &PanelDefinition,
    placement: &PanelPlacementConfig,
) -> (i32, u8, String) {
    let rank = match def.kind {
        PanelKind::BaseFieldsSlice { .. } => 1,
        _ => owner_rank(&def.owner),
    };
    (placement.order, rank, def.panel_id.clone())
}

pub(crate) fn has_core_body_panel(effective: &EffectiveContentSchema) -> bool {
    let type_key = effective.base.descriptor.key.as_str();
    let is_builtin = matches!(type_key, "post" | "page");
    let settings = &effective.base.descriptor.settings;
    !matches!(type_key, "section" | "pattern")
        && (is_builtin
            || settings.supports(ContentSupport::Editor)
            || settings.supports(ContentSupport::Builder))
        && effective
            .effective_fields
            .get("body")
            .is_some_and(|fe| fe.definition.field_type == ContentFieldType::RichText)
}

pub fn eligible_base_field_keys(effective: &EffectiveContentSchema) -> Vec<String> {
    let type_key = effective.base.descriptor.key.as_str();
    let use_body_panel = has_core_body_panel(effective);
    let is_private_reusable = matches!(type_key, "section" | "pattern");
    effective
        .base
        .descriptor
        .fields
        .iter()
        .filter(|f| {
            !(use_body_panel && f.key == "body")
                && !(!is_private_reusable && f.key == "featured_image")
        })
        .map(|f| f.key.clone())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutValidationError {
    pub code: &'static str,
    pub message: String,
}

impl LayoutValidationError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: format!("{code}: {}", message.into()),
        }
    }
}
