use super::*;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]

pub struct FieldSpec {
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions: Option<ConditionGroups>,

    // Context-specific capability flags
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acl: Option<FieldAcl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_state: Option<FieldDeclarationState>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub show_in_list: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub public: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub translatable: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub responsive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css_var: Option<String>,

    // Text / Textarea / Richtext / Number / Range / Date / Spacing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolbar: Option<RichTextToolbar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<CssUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<SpacingProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_date: Option<String>,

    // Choice / Boolean / Color / Media / Gallery / Relational
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SelectOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_source: Option<SelectOptionsSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<SelectDisplay>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multiple: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_null: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_label: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_alpha: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_kind: Option<MediaFieldKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy_from_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picker_style: Option<TermPickerStyle>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub sync_entry_terms: bool,

    // Group / Repeater
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_label_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button_label: Option<String>,

    // UI-only layout primitives (tab / accordion / message)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<FieldLayoutPlacement>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub open_by_default: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multi_expand: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub endpoint: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<MessageTone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

impl FieldSpec {
    pub fn new(field_type: FieldType, key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            field_type,
            key: key.into(),
            label: label.into(),
            instructions: None,
            placeholder: None,
            required: false,
            default: None,
            width: None,
            conditions: None,
            acl: None,
            declaration_state: None,
            show_in_list: false,
            public: false,
            translatable: false,
            responsive: false,
            css_var: None,
            min_chars: None,
            max_chars: None,
            max_bytes: None,
            rows: None,
            prepend: None,
            append: None,
            toolbar: None,
            min: None,
            max: None,
            step: None,
            unit: None,
            property: None,
            min_date: None,
            max_date: None,
            options: Vec::new(),
            options_source: None,
            display: None,
            multiple: false,
            allow_null: false,
            on_label: None,
            off_label: None,
            allow_alpha: false,
            media_kind: None,
            min_items: None,
            max_items: None,
            target_types: Vec::new(),
            taxonomy: None,
            taxonomy_from_field: None,
            picker_style: None,
            sync_entry_terms: false,
            fields: Vec::new(),
            layout: None,
            item_label_field: None,
            button_label: None,
            placement: None,
            open_by_default: false,
            multi_expand: false,
            endpoint: false,
            tone: None,
            body: None,
        }
    }

    pub fn is_ui_only(&self) -> bool {
        self.field_type.is_ui_only()
    }

    pub fn is_withdrawn(&self) -> bool {
        self.declaration_state == Some(FieldDeclarationState::Withdrawn)
    }

    pub fn effective_required(&self) -> bool {
        self.required && !self.is_withdrawn() && !self.is_ui_only()
    }

    pub fn effective_media_kind(&self) -> MediaFieldKind {
        self.media_kind.unwrap_or(MediaFieldKind::Image)
    }

    pub fn effective_spacing_property(&self) -> SpacingProperty {
        self.property.unwrap_or(SpacingProperty::Padding)
    }

    pub fn effective_unit(&self) -> CssUnit {
        self.unit.unwrap_or(CssUnit::Px)
    }

    pub fn effective_acl(&self) -> (FieldRole, FieldRole, PublicExposure) {
        match &self.acl {
            Some(acl) => (
                acl.min_view_role.unwrap_or(FieldRole::Editor),
                acl.min_edit_role.unwrap_or(FieldRole::Editor),
                acl.public_exposure.unwrap_or(PublicExposure::Public),
            ),
            None => (FieldRole::Editor, FieldRole::Editor, PublicExposure::Public),
        }
    }

    pub fn checkbox_layout(&self) -> Result<Option<CheckboxLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::Checkbox {
            return Err("Thuộc tính layout 'checkbox' không khớp kiểu trường.");
        }
        match raw {
            "vertical" => Ok(Some(CheckboxLayout::Vertical)),
            "horizontal" => Ok(Some(CheckboxLayout::Horizontal)),
            _ => Err("layout của checkbox chỉ chấp nhận 'vertical' hoặc 'horizontal'."),
        }
    }

    pub fn radio_layout(&self) -> Result<Option<RadioLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::RadioButtons {
            return Err("Thuộc tính layout 'radio_buttons' không khớp kiểu trường.");
        }
        match raw {
            "buttons" => Ok(Some(RadioLayout::Buttons)),
            "radio" => Ok(Some(RadioLayout::Radio)),
            _ => Err("layout của radio_buttons chỉ chấp nhận 'buttons' hoặc 'radio'."),
        }
    }

    pub fn repeater_layout(&self) -> Result<Option<RepeaterLayout>, &'static str> {
        let Some(raw) = self.layout.as_deref() else {
            return Ok(None);
        };
        if self.field_type != FieldType::Repeater {
            return Err("Thuộc tính layout 'repeater' không khớp kiểu trường.");
        }
        match raw {
            "accordion" => Ok(Some(RepeaterLayout::Accordion)),
            "table" => Ok(Some(RepeaterLayout::Table)),
            "block" => Ok(Some(RepeaterLayout::Block)),
            _ => Err("layout của repeater chỉ chấp nhận 'accordion', 'table' hoặc 'block'."),
        }
    }
}
