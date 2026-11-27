use super::*;

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn execute_host_read_request(
    db: &SqlitePool,
    plugin_id: &str,
    manifest: &PluginManifest,
    execution_mode: HookExecutionMode,
    principal: ExecutionPrincipal,
    action_caps: Option<&[String]>,
    secrets_root: Option<&Path>,
    req: HostReadRequest,
) -> HostOperationResponse {
    execute_host_read_request_for_owner(
        db,
        ExtensionOwnerKind::Plugin,
        plugin_id,
        manifest,
        execution_mode,
        principal,
        action_caps,
        secrets_root,
        req,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_entry_for_host(
    owner_kind: ExtensionOwnerKind,
    manifest: &PluginManifest,
    action_caps: Option<&[String]>,
    actor_role: FieldRole,
    is_public_scope: bool,
    type_key: &str,
    effective: &opendash_core::content::EffectiveContentSchema,
    entry: entries::ContentEntryRecord,
) -> Value {
    let mut filtered_values = BTreeMap::new();
    for (k, v) in entry.values {
        if let Some(fe) = effective.effective_fields.get(&k) {
            let allowed = if is_public_scope {
                fe.definition.effective_acl().2 == PublicExposure::Public
                    && plugin_can_read_field_for_owner(
                        owner_kind,
                        manifest,
                        action_caps,
                        actor_role,
                        type_key,
                        fe,
                    )
            } else {
                plugin_can_read_field_for_owner(
                    owner_kind,
                    manifest,
                    action_caps,
                    actor_role,
                    type_key,
                    fe,
                )
            };
            if allowed {
                filtered_values.insert(k, v);
            }
        }
    }
    json!({
        "id": entry.id,
        "type_key": entry.content_type_key,
        "slug": entry.slug,
        "title": entry.title,
        "status": entry.publication_status,
        "version": entry.version,
        "locale": entry.locale,
        "values": filtered_values,
        "term_ids": entry.term_ids,
    })
}
