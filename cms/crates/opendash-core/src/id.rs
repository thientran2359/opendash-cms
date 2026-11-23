//! Canonical identifier, key, namespace, and version validation primitives.
//!
//! All modules across `opendash-core` and `opendash-server` MUST use these
//! functions instead of defining ad-hoc identifier validators.

/// Validates a `snake_case` identifier (`[a-z][a-z0-9_]*`) within `1..=max_len`.
pub fn valid_snake_key_with_limit(value: &str, max_len: usize) -> bool {
    (1..=max_len).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Validates a standard `snake_case` key (`1..=64` chars) used for CPT keys,
/// field keys, and internal schema identifiers.
pub fn valid_snake_key(value: &str) -> bool {
    valid_snake_key_with_limit(value, 64)
}

/// Validates a `kebab-case` segment (`[a-z][a-z0-9-]*`) within `1..=max_len`.
pub fn valid_kebab_segment_with_limit(value: &str, max_len: usize) -> bool {
    (1..=max_len).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub const MAX_BCP47_TAG_BYTES: usize = 35;

/// Validates an ASCII BCP47 language tag (`primary` 2–8 ASCII letters, optional
/// 1–8 ASCII alphanumeric subtags separated by `-`, up to 35 bytes).
pub fn valid_bcp47_tag(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_BCP47_TAG_BYTES {
        return false;
    }
    let mut parts = value.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    if !(2..=8).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|sub| (1..=8).contains(&sub.len()) && sub.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Validates a lowercase canonical locale code derived from a BCP47 language tag.
pub fn valid_lowercase_locale_code(value: &str) -> bool {
    valid_bcp47_tag(value) && !value.bytes().any(|b| b.is_ascii_uppercase())
}

/// Validates a namespaced block identifier (`<namespace>/<name>`) where each
/// segment is a `kebab-case` identifier up to 32 characters (e.g. `sample-theme/hero-banner`).
pub fn valid_namespaced_block_id(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    valid_kebab_segment_with_limit(namespace, 32) && valid_kebab_segment_with_limit(name, 32)
}

/// Validates a namespaced schema key (`<namespace>/<name>`) where each segment
/// is a `snake_case` key up to 64 characters (e.g. `core/heading`).
pub fn valid_namespaced_snake_key(value: &str) -> bool {
    let Some((namespace, name)) = value.split_once('/') else {
        return false;
    };
    valid_snake_key(namespace) && valid_snake_key(name)
}

/// Validates a package identifier (plugin or theme ID, `3..=64` chars, `[a-z0-9-]`,
/// not starting or ending with `-`, and not a reserved system name).
pub fn validate_package_id(id: &str) -> Result<(), &'static str> {
    if id.len() < 3 || id.len() > 64 {
        return Err("ID plugin phải từ 3 đến 64 ký tự.");
    }
    if matches!(id, "core" | "legacy") {
        return Err("ID gói không được dùng tên dành riêng của hệ thống.");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("ID plugin chỉ được chứa chữ thường, số và dấu gạch ngang.");
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err("ID plugin không được bắt đầu hoặc kết thúc bằng dấu gạch ngang.");
    }
    Ok(())
}

/// Boolean helper for package ID validation.
pub fn valid_package_id(value: &str) -> bool {
    validate_package_id(value).is_ok()
}

/// Validates a SemVer 2.0.0 version string (`MAJOR.MINOR.PATCH` with optional
/// pre-release and build metadata).
pub fn validate_semver(version: &str) -> bool {
    if version.is_empty() || version.len() > 64 {
        return false;
    }
    let mut parts = version.splitn(2, '+');
    let main_and_pre = parts.next().unwrap_or("");
    if let Some(build) = parts.next() {
        if build.split('.').any(|part| part.is_empty())
            || !build
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            return false;
        }
    }
    let mut pre_parts = main_and_pre.splitn(2, '-');
    let numbers_part = pre_parts.next().unwrap_or("");
    if let Some(pre) = pre_parts.next() {
        if pre.split('.').any(|part| {
            part.is_empty()
                || (part.len() > 1
                    && part.starts_with('0')
                    && part.bytes().all(|b| b.is_ascii_digit()))
        }) || !pre
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        {
            return false;
        }
    }
    let nums: Vec<&str> = numbers_part.split('.').collect();
    if nums.len() != 3 {
        return false;
    }
    for num in nums {
        if num.is_empty() || (num.len() > 1 && num.starts_with('0')) {
            return false;
        }
        if !num.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// Validates a layout-scoped base-field slice panel identifier (`layout:<slice_key>`),
/// returning the inner `<slice_key>` when valid.
pub fn valid_layout_slice_panel_id(value: &str) -> Option<&str> {
    let slice_key = value.strip_prefix("layout:")?;
    if valid_snake_key(slice_key) {
        Some(slice_key)
    } else {
        None
    }
}

/// Structured identifier for field groups and editor panels (`owner` + `local_key`),
/// keeping `PackageId` (`kebab-case`) separate from `local_key` (`snake_case`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScopedResourceId {
    pub owner: crate::registry::ResourceOwner,
    pub local_key: String,
}

impl std::hash::Hash for ScopedResourceId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.owner {
            crate::registry::ResourceOwner::Core => {
                0u8.hash(state);
            }
            crate::registry::ResourceOwner::User => {
                1u8.hash(state);
            }
            crate::registry::ResourceOwner::Theme(id) => {
                2u8.hash(state);
                id.hash(state);
            }
            crate::registry::ResourceOwner::Plugin(id) => {
                3u8.hash(state);
                id.hash(state);
            }
        }
        self.local_key.hash(state);
    }
}

impl ScopedResourceId {
    pub fn new(
        owner: crate::registry::ResourceOwner,
        local_key: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let local_key = local_key.into();
        if !valid_snake_key(&local_key) {
            return Err(
                "local_key trong ScopedResourceId phải là snake_case hợp lệ (1..=64 ký tự).",
            );
        }
        match &owner {
            crate::registry::ResourceOwner::Core | crate::registry::ResourceOwner::User => {}
            crate::registry::ResourceOwner::Theme(pkg_id)
            | crate::registry::ResourceOwner::Plugin(pkg_id) => {
                if !valid_package_id(pkg_id) {
                    return Err(
                        "owner_id trong ScopedResourceId phải là package_id hợp lệ (3..=64 ký tự).",
                    );
                }
            }
        }
        Ok(Self { owner, local_key })
    }

    pub fn to_canonical(&self) -> String {
        match &self.owner {
            crate::registry::ResourceOwner::Core => format!("core:{}", self.local_key),
            crate::registry::ResourceOwner::User => format!("user:{}", self.local_key),
            crate::registry::ResourceOwner::Theme(pkg_id) => {
                format!("theme:{pkg_id}:{}", self.local_key)
            }
            crate::registry::ResourceOwner::Plugin(pkg_id) => {
                format!("plugin:{pkg_id}:{}", self.local_key)
            }
        }
    }

    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.starts_with("layout:") {
            return Err(
                "reserved_panel_namespace: Tiền tố 'layout:' chỉ dành riêng cho cấu hình bố cục cấp site.",
            );
        }
        let parts: Vec<&str> = raw.split(':').collect();
        match parts.as_slice() {
            ["core", local_key] => Self::new(crate::registry::ResourceOwner::Core, *local_key),
            ["user", local_key] => Self::new(crate::registry::ResourceOwner::User, *local_key),
            ["theme", pkg_id, local_key] => Self::new(
                crate::registry::ResourceOwner::Theme((*pkg_id).to_owned()),
                *local_key,
            ),
            ["plugin", pkg_id, local_key] => Self::new(
                crate::registry::ResourceOwner::Plugin((*pkg_id).to_owned()),
                *local_key,
            ),
            _ => Err("Định danh ScopedResourceId không đúng định dạng chuẩn."),
        }
    }
}

impl std::fmt::Display for ScopedResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_canonical())
    }
}

impl std::str::FromStr for ScopedResourceId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl serde::Serialize for ScopedResourceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_canonical())
    }
}

impl<'de> serde::Deserialize<'de> for ScopedResourceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Canonical(String),
            Structured {
                owner: crate::registry::ResourceOwner,
                local_key: String,
            },
        }

        match Repr::deserialize(deserializer)? {
            Repr::Canonical(s) => Self::parse(&s).map_err(serde::de::Error::custom),
            Repr::Structured { owner, local_key } => {
                Self::new(owner, local_key).map_err(serde::de::Error::custom)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_snake_and_kebab_identifiers() {
        assert!(valid_snake_key("post"));
        assert!(valid_snake_key("hero_image_1"));
        assert!(!valid_snake_key("hero-image"));
        assert!(!valid_snake_key("1post"));

        assert!(valid_namespaced_block_id("sample-theme/hero-banner"));
        assert!(!valid_namespaced_block_id("sample_theme/hero_banner"));
        assert!(valid_namespaced_snake_key("core/heading_block"));
    }

    #[test]
    fn validates_package_id_and_semver() {
        assert!(valid_package_id("sample-plugin"));
        assert!(!valid_package_id("ab"));
        assert!(!valid_package_id("-sample"));
        assert!(validate_semver("1.0.0"));
        assert!(validate_semver("2.1.3-beta.1+exp.sha"));
        assert!(!validate_semver("1.0"));
        for bad in [
            "1.0.0-alpha..1",
            "1.0.0-01",
            "1.0.0+build..x",
            "1.0.0-",
            "1.0.0+",
            "01.0.0",
        ] {
            assert!(!validate_semver(bad), "{bad}");
        }
        assert!(validate_semver("1.0.0-0a+001"));
        assert!(valid_namespaced_block_id("legacy-/block-")); // M1 compatibility
        assert!(!valid_package_id("legacy-"));
        assert!(!valid_package_id("core"));
        assert!(!valid_package_id("legacy"));
        assert!(valid_namespaced_block_id("core/banner"));
    }

    #[test]
    fn parses_and_serializes_scoped_resource_id() {
        let user_id = ScopedResourceId::parse("user:project_meta").unwrap();
        assert_eq!(user_id.owner, crate::registry::ResourceOwner::User);
        assert_eq!(user_id.local_key, "project_meta");
        assert_eq!(user_id.to_canonical(), "user:project_meta");

        let theme_id = ScopedResourceId::parse("theme:moc-studio:hero_fields").unwrap();
        assert_eq!(
            theme_id.owner,
            crate::registry::ResourceOwner::Theme("moc-studio".into())
        );
        assert_eq!(theme_id.local_key, "hero_fields");
        assert_eq!(theme_id.to_canonical(), "theme:moc-studio:hero_fields");

        let plugin_id = ScopedResourceId::parse("plugin:seo-pro:extra_meta").unwrap();
        assert_eq!(
            plugin_id.owner,
            crate::registry::ResourceOwner::Plugin("seo-pro".into())
        );
        assert_eq!(plugin_id.local_key, "extra_meta");
        assert_eq!(plugin_id.to_canonical(), "plugin:seo-pro:extra_meta");

        assert!(ScopedResourceId::parse("layout:project_info").is_err());
        assert_eq!(
            valid_layout_slice_panel_id("layout:project_info"),
            Some("project_info")
        );
        assert_eq!(valid_layout_slice_panel_id("layout:bad-key"), None);
    }
}
