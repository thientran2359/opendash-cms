use super::*;

pub const MAX_CUSTOM_OPTIONS_PAGES_PER_SITE: usize = 64;
pub const MAX_CUSTOM_OPTIONS_PAGES_PER_PACKAGE: usize = 16;
pub const MAX_OPTIONS_PAGE_DESCRIPTION_CHARS: usize = 400;

pub const RESERVED_OPTIONS_PAGE_KEYS: &[&str] = &[
    "site_global",
    "theme",
    "options",
    "general",
    "multilingual",
    "permalinks",
    "reading",
    "seo",
    "smtp",
    "contact",
];

pub fn valid_custom_options_page_key(key: &str) -> Result<(), &'static str> {
    if !valid_snake_key(key) {
        return Err("Khóa trang tùy chọn phải là snake_case hợp lệ (1..=64 ký tự).");
    }
    if crate::id::is_reserved_system_root(key) || RESERVED_OPTIONS_PAGE_KEYS.contains(&key) {
        return Err(
            "options_page_key_reserved: Khóa trang tùy chọn trùng với tên hệ thống được bảo lưu.",
        );
    }
    Ok(())
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum OptionsPageParentMenu {
    Content,
    Appearance,
    System,
    #[default]
    Settings,
    TopLevel,
}

impl OptionsPageParentMenu {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Appearance => "appearance",
            Self::System => "system",
            Self::Settings => "settings",
            Self::TopLevel => "top_level",
        }
    }

    pub fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "content" => Some(Self::Content),
            "appearance" => Some(Self::Appearance),
            "system" => Some(Self::System),
            "settings" => Some(Self::Settings),
            "top_level" => Some(Self::TopLevel),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomOptionsPageDeclaration {
    pub page_key: String,
    pub title: String,
    pub menu_label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_icon: Option<String>,
    #[serde(default = "default_panel_order_50")]
    pub menu_position: i32,
    #[serde(default)]
    pub parent_menu: OptionsPageParentMenu,
    #[serde(default)]
    pub min_edit_role: FieldRole,
}

impl CustomOptionsPageDeclaration {
    pub fn validate_and_normalize(&mut self) -> Result<(), &'static str> {
        valid_custom_options_page_key(&self.page_key)?;
        if !valid_label(&self.title) || !valid_label(&self.menu_label) {
            return Err("Tiêu đề hoặc nhãn menu của trang tùy chọn không hợp lệ.");
        }
        if self.description.chars().count() > MAX_OPTIONS_PAGE_DESCRIPTION_CHARS
            || self
                .description
                .chars()
                .any(|c| c.is_control() && c != '\n')
        {
            return Err("Mô tả trang tùy chọn tối đa 400 ký tự và không chứa ký tự điều khiển.");
        }
        if let Some(raw_icon) = self.menu_icon.as_deref() {
            let trimmed = raw_icon.trim();
            if trimmed.is_empty() {
                self.menu_icon = None;
            } else if crate::icons::lookup_core_icon_paths(trimmed).is_none() {
                return Err("menu_icon của trang tùy chọn phải thuộc danh mục CORE_ICONS.");
            } else {
                self.menu_icon = Some(trimmed.to_owned());
            }
        }
        if !(0..=1000).contains(&self.menu_position) {
            return Err("menu_position của trang tùy chọn phải nằm trong 0..=1000.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomOptionsPageInput {
    pub page_key: String,
    pub title: String,
    pub menu_label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub menu_icon: Option<String>,
    #[serde(default = "default_panel_order_50")]
    pub menu_position: i32,
    #[serde(default)]
    pub parent_menu: OptionsPageParentMenu,
    #[serde(default)]
    pub min_edit_role: FieldRole,
}

impl CustomOptionsPageInput {
    pub fn validate_and_normalize(&mut self) -> Result<(), &'static str> {
        let mut decl = CustomOptionsPageDeclaration {
            page_key: self.page_key.clone(),
            title: self.title.clone(),
            menu_label: self.menu_label.clone(),
            description: self.description.clone(),
            menu_icon: self.menu_icon.clone(),
            menu_position: self.menu_position,
            parent_menu: self.parent_menu,
            min_edit_role: self.min_edit_role,
        };
        decl.validate_and_normalize()?;
        self.menu_icon = decl.menu_icon;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomOptionsPageDefinition {
    pub page_key: String,
    pub owner: ResourceOwner,
    pub title: String,
    pub menu_label: String,
    pub description: String,
    pub menu_icon: Option<String>,
    pub menu_position: i32,
    pub parent_menu: OptionsPageParentMenu,
    pub min_edit_role: FieldRole,
    pub provider_status: ProviderStatus,
    pub declaration_state: DeclarationState,
    pub version: i64,
}
