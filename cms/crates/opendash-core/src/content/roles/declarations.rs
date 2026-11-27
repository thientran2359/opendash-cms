use super::*;

impl CntDeclarations {
    pub fn is_empty(&self) -> bool {
        self.content_types.is_empty()
            && self.taxonomy_definitions.is_empty()
            && self.taxonomy_eligibility.is_empty()
            && self.field_groups.is_empty()
            && self.options_pages.is_empty()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.taxonomy_definitions.len() > 64 {
            return Err("Mỗi gói hỗ trợ tối đa 64 taxonomy_definitions.");
        }
        let mut seen_taxonomies = HashSet::new();
        for taxonomy in &self.taxonomy_definitions {
            taxonomy.validate()?;
            if !seen_taxonomies.insert(taxonomy.key.as_str()) {
                return Err("Khóa taxonomy bị trùng lặp trong declarations.");
            }
        }
        let mut seen_keys = HashSet::new();
        for ct in &self.content_types {
            if !seen_keys.insert(ct.key.as_str()) {
                return Err("Khóa content_type bị trùng lặp trong declarations.");
            }
            if matches!(ct.key.as_str(), "post" | "page" | "pattern" | "section")
                && ct.rewrite_base.is_some()
            {
                return Err("Core Post/Page/Pattern không hỗ trợ rewrite_base riêng của provider.");
            }
            validate_provider_rewrite_base(ct.rewrite_base.as_deref())?;
            let mut settings = ct.settings.clone().unwrap_or_default();
            settings.validate_and_normalize()?;
            let def = ContentTypeDefinition {
                key: ct.key.clone(),
                label: ct.label.clone(),
                singular_label: ct.singular_label.clone(),
                description: ct.description.clone(),
                settings,
            };
            def.validate()?;
            validate_provider_taxonomy_keys(&ct.taxonomies)?;
        }
        let mut seen_eligibility = HashSet::new();
        for elig in &self.taxonomy_eligibility {
            if !valid_content_type_key(&elig.content_type) {
                return Err("Khóa loại nội dung trong taxonomy_eligibility không hợp lệ.");
            }
            validate_provider_taxonomy_keys(&elig.taxonomies)?;
            for tax in &elig.taxonomies {
                if !seen_eligibility.insert((elig.content_type.as_str(), tax.as_str())) {
                    return Err("Trùng association content_type/taxonomy trong declarations.");
                }
            }
        }
        if self.field_groups.len() > 16 {
            return Err("Mỗi gói hỗ trợ tối đa 16 field_groups trong declarations.");
        }
        let mut seen_groups = HashSet::new();
        for fg in &self.field_groups {
            if !seen_groups.insert(fg.local_key.as_str()) {
                return Err("duplicate_panel_id: Trùng local_key trong danh sách field_groups.");
            }
            fg.validate_and_normalize()?;
        }
        if self.options_pages.len() > MAX_CUSTOM_OPTIONS_PAGES_PER_PACKAGE {
            return Err("Mỗi gói hỗ trợ tối đa 16 options_pages trong declarations.");
        }
        let mut seen_pages = HashSet::new();
        for page in &self.options_pages {
            let mut copy = page.clone();
            copy.validate_and_normalize()?;
            if !seen_pages.insert(page.page_key.as_str()) {
                return Err("Khóa page_key trong options_pages bị trùng lặp.");
            }
        }
        Ok(())
    }
}

/// References may point to core, site-owned, or same-provider taxonomies;
/// canonical storage verifies existence, ownership, and provider activity.
pub(crate) fn validate_provider_taxonomy_keys(keys: &[String]) -> Result<(), &'static str> {
    if keys.len() > 64 {
        return Err("Mỗi content type hỗ trợ tối đa 64 taxonomy associations.");
    }
    let mut seen = HashSet::new();
    for key in keys {
        if !valid_content_type_key(key) {
            return Err("Khóa taxonomy trong eligibility không hợp lệ.");
        }
        if !seen.insert(key.as_str()) {
            return Err("Khóa taxonomy trùng lặp trong eligibility.");
        }
    }
    Ok(())
}

impl TaxonomyDefinitionInput {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_content_type_key(&self.key) {
            return Err("Khóa taxonomy không hợp lệ.");
        }
        if matches!(
            self.key.as_str(),
            "category" | "tag" | "post" | "page" | "pattern" | "section"
        ) || RESERVED_ENTRY_KEYS.contains(&self.key.as_str())
        {
            return Err("Khóa taxonomy thuộc namespace Core được bảo vệ.");
        }
        if !valid_label(&self.label) {
            return Err("Tên taxonomy không hợp lệ (tối đa 100 ký tự).");
        }
        if let Some(mut settings) = self.settings.clone() {
            settings.validate_and_normalize()?;
        }
        if let Some(cts) = &self.content_types {
            validate_taxonomy_target_cpts(cts)?;
        }
        if let PatchField::Set(base) = &self.rewrite_base
            && !base.trim().is_empty()
        {
            validate_provider_rewrite_base(Some(base.trim()))?;
        }
        Ok(())
    }
}
