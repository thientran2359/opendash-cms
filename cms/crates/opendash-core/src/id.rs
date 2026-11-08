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
/// not starting or ending with `-`).
pub fn validate_package_id(id: &str) -> Result<(), &'static str> {
    if id.len() < 3 || id.len() > 64 {
        return Err("ID plugin phải từ 3 đến 64 ký tự.");
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
    }
}
