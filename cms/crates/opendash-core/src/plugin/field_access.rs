use super::*;

pub const CURRENT_HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(super) fn parse_core_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn host_satisfies_min_version(host: &str, min: &str) -> bool {
    match (parse_core_semver(host), parse_core_semver(min)) {
        (Some(h), Some(m)) => h >= m,
        _ => false,
    }
}

pub(crate) fn validate_min_host_version(
    min_host_version: Option<&str>,
) -> Result<(), &'static str> {
    if let Some(min_ver) = min_host_version {
        if !validate_semver(min_ver) {
            return Err("min_host_version không đúng định dạng SemVer 2.0.0.");
        }
        if !host_satisfies_min_version(CURRENT_HOST_VERSION, min_ver) {
            return Err("Phiên bản host hiện tại thấp hơn min_host_version yêu cầu.");
        }
    }
    Ok(())
}

pub(crate) fn validate_hooks_and_filters(
    hooks: &[crate::hooks::HookEvent],
    filters: &[crate::hooks::HookEvent],
    allow_theme_after_activate: bool,
) -> Result<(), &'static str> {
    let mut seen_hooks = HashSet::new();
    for event in hooks {
        if *event == crate::hooks::HookEvent::PluginManualRun
            || event.is_filter()
            || (!allow_theme_after_activate
                && *event == crate::hooks::HookEvent::ThemeAfterActivate)
        {
            return Err("Sự kiện hook không được hỗ trợ trong danh sách hooks.");
        }
        if !seen_hooks.insert(event.as_str()) {
            return Err("Danh sách hooks không được chứa sự kiện trùng lặp.");
        }
    }
    let mut seen_filters = HashSet::new();
    for filter in filters {
        if !filter.is_filter() {
            return Err("Chỉ các filter:* mới được khai báo trong danh sách filters.");
        }
        if !seen_filters.insert(filter.as_str()) {
            return Err("Danh sách filters không được chứa sự kiện trùng lặp.");
        }
    }
    Ok(())
}

pub(crate) fn validate_settings_schema(
    settings_schema: &[crate::field_schema::FieldSpec],
) -> Result<HashSet<&str>, &'static str> {
    crate::field_schema::validate_field_specs(
        settings_schema,
        crate::field_schema::FieldContext::OwnerSettings,
    )?;
    let mut secret_keys = HashSet::new();
    for field in settings_schema {
        if field.field_type == crate::field_schema::FieldType::Secret {
            secret_keys.insert(field.key.as_str());
        }
    }
    Ok(secret_keys)
}

/// Checks whether an extension (`owner_kind` Plugin or Theme) is permitted to read
/// `field_entry` for `actor_role`.
pub fn plugin_can_read_field_for_owner(
    owner_kind: crate::hooks::ExtensionOwnerKind,
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    if !manifest.can_read_content_type(type_key) {
        return false;
    }
    if let Some(caps) = action_caps {
        if !caps.iter().any(|c| c == "content_read") {
            return false;
        }
    }
    if actor_role < field_entry.definition.min_view_role {
        return false;
    }
    if field_entry.definition.public_exposure == crate::content::PublicExposure::Public {
        return true;
    }
    let expected_owner = match owner_kind {
        crate::hooks::ExtensionOwnerKind::Plugin => {
            crate::registry::ResourceOwner::Plugin(manifest.id.clone())
        }
        crate::hooks::ExtensionOwnerKind::Theme => {
            crate::registry::ResourceOwner::Theme(manifest.id.clone())
        }
    };
    if field_entry.owner == expected_owner {
        return true;
    }
    manifest.has_private_read_grant(type_key, &field_entry.definition.key)
        && action_caps.is_none_or(|caps| caps.iter().any(|c| c == "content_read_private"))
}

/// Checks whether a plugin (in hook mode `action_caps == None` or action mode `Some(caps)`)
/// is permitted to read `field_entry` for `actor_role` (`270-R3`).
pub fn plugin_can_read_field(
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    plugin_can_read_field_for_owner(
        crate::hooks::ExtensionOwnerKind::Plugin,
        manifest,
        action_caps,
        actor_role,
        type_key,
        field_entry,
    )
}

/// Checks whether an extension (`owner_kind` Plugin or Theme) is permitted to write
/// `field_entry` for `actor_role`.
pub fn plugin_can_write_field_for_owner(
    owner_kind: crate::hooks::ExtensionOwnerKind,
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    if !manifest.can_write_content_type(type_key) {
        return false;
    }
    if let Some(caps) = action_caps {
        if !caps.iter().any(|c| c == "content_write") {
            return false;
        }
    }
    if actor_role < field_entry.definition.min_edit_role {
        return false;
    }
    if field_entry.definition.public_exposure == crate::content::PublicExposure::Public {
        return true;
    }
    let expected_owner = match owner_kind {
        crate::hooks::ExtensionOwnerKind::Plugin => {
            crate::registry::ResourceOwner::Plugin(manifest.id.clone())
        }
        crate::hooks::ExtensionOwnerKind::Theme => {
            crate::registry::ResourceOwner::Theme(manifest.id.clone())
        }
    };
    if field_entry.owner == expected_owner {
        return true;
    }
    manifest.has_private_write_grant(type_key, &field_entry.definition.key)
        && action_caps.is_none_or(|caps| caps.iter().any(|c| c == "content_write_private"))
}

/// Checks whether a plugin (in hook/filter mode `action_caps == None` or action mode `Some(caps)`)
/// is permitted to write `field_entry` for `actor_role` (`270-R3`).
pub fn plugin_can_write_field(
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: crate::content::FieldRole,
    type_key: &str,
    field_entry: &crate::content::EffectiveFieldEntry,
) -> bool {
    plugin_can_write_field_for_owner(
        crate::hooks::ExtensionOwnerKind::Plugin,
        manifest,
        action_caps,
        actor_role,
        type_key,
        field_entry,
    )
}

pub const PLUGIN_ABI_VERSION: u32 = 1;
