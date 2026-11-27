use super::*;
#[allow(clippy::too_many_arguments)]

pub async fn execute_host_read_request_for_owner(
    db: &SqlitePool,
    owner_kind: ExtensionOwnerKind,
    plugin_id: &str,
    manifest: &PluginManifest,
    execution_mode: HookExecutionMode,
    principal: ExecutionPrincipal,
    action_caps: Option<&[String]>,
    secrets_root: Option<&Path>,
    req: HostReadRequest,
) -> HostOperationResponse {
    if let Err(msg) =
        host_ops::check_read_policy(manifest, execution_mode, principal, action_caps, &req)
    {
        return HostOperationResponse::err(msg);
    }

    let actor_role = match principal {
        ExecutionPrincipal::Admin | ExecutionPrincipal::System => FieldRole::Admin,
        ExecutionPrincipal::Editor | ExecutionPrincipal::Public => FieldRole::Editor,
    };
    let is_public_scope = principal == ExecutionPrincipal::Public
        || execution_mode == HookExecutionMode::FilterProjection;

    match req {
        HostReadRequest::KvGet { key } => {
            if key.is_empty() || key.len() > host_ops::MAX_KV_KEY_BYTES {
                return HostOperationResponse::err("Key không hợp lệ hoặc quá dài.");
            }
            match plugin_kv::get_for_owner(db, owner_kind, plugin_id, &key).await {
                Ok(Some(val_str)) => match serde_json::from_str::<Value>(&val_str) {
                    Ok(v) => HostOperationResponse {
                        ok: true,
                        error: None,
                        data: None,
                        found: Some(true),
                        value: Some(v),
                        status: None,
                        headers: None,
                        body: None,
                    },
                    Err(_) => {
                        HostOperationResponse::err("Dữ liệu trong database không phải JSON hợp lệ.")
                    }
                },
                Ok(None) => HostOperationResponse {
                    ok: true,
                    error: None,
                    data: None,
                    found: Some(false),
                    value: None,
                    status: None,
                    headers: None,
                    body: None,
                },
                Err(e) => HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
            }
        }
        HostReadRequest::KvList { prefix, limit } => {
            let prefix_str = prefix.unwrap_or_default();
            let clamped = limit.unwrap_or(50).clamp(1, 200) as i64;
            match plugin_kv::list_for_owner(db, owner_kind, plugin_id, &prefix_str, clamped).await {
                Ok(rows) => {
                    let mut items = Vec::with_capacity(rows.len());
                    for (k, v_json) in rows {
                        let v = serde_json::from_str::<Value>(&v_json).unwrap_or(Value::Null);
                        items.push(json!({ "key": k, "value": v }));
                    }
                    HostOperationResponse::ok_data(json!({ "items": items }))
                }
                Err(e) => HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
            }
        }
        HostReadRequest::ContentGet {
            type_key,
            entry_id,
            locale,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let effective_opt =
                match crate::storage::content::load_effective_content_schema(db, &type_key).await {
                    Ok(opt) => opt,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(effective) = effective_opt else {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            };
            if is_public_scope
                && (effective.base.status != ProviderStatus::Active
                    || !effective.base.descriptor.settings.public)
            {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            }
            let entry_opt =
                match plugin_storage::get_entry_for_host(db, &type_key, entry_id, is_public_scope)
                    .await
                {
                    Ok(e) => e,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(entry) = entry_opt else {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            };
            if is_public_scope && entry.publication_status != "published" {
                return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
            }
            if let Some(req_loc) = locale {
                let normalized = match settings::resolve_requested_locale(db, Some(&req_loc)).await
                {
                    Ok(l) => l,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
                if entry.locale != normalized {
                    return HostOperationResponse::ok_data(json!({ "entry": Value::Null }));
                }
            }
            let view = project_entry_for_host(
                owner_kind,
                manifest,
                action_caps,
                actor_role,
                is_public_scope,
                &type_key,
                &effective,
                entry,
            );
            HostOperationResponse::ok_data(json!({ "entry": view }))
        }
        HostReadRequest::ContentList {
            type_key,
            locale,
            limit,
            offset,
            published_only,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let effective_opt =
                match crate::storage::content::load_effective_content_schema(db, &type_key).await {
                    Ok(opt) => opt,
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                };
            let Some(effective) = effective_opt else {
                return HostOperationResponse::ok_data(json!({ "items": [], "total": 0 }));
            };
            if is_public_scope
                && (effective.base.status != ProviderStatus::Active
                    || !effective.base.descriptor.settings.public)
            {
                return HostOperationResponse::ok_data(json!({ "items": [], "total": 0 }));
            }
            let resolved_locale = match locale {
                Some(raw) => match settings::resolve_requested_locale(db, Some(&raw)).await {
                    Ok(l) => Some(l),
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                },
                None => None,
            };
            let clamped_limit = limit.unwrap_or(20).clamp(1, 50) as i64;
            let clamped_offset = offset.unwrap_or(0).min(10_000) as i64;
            let eff_published = is_public_scope || published_only.unwrap_or(false);
            let filters = entries::EntryFilters {
                category_id: None,
                tag_id: None,
                taxonomy_key: None,
                term_id: None,
                include_descendants: false,
                locale: resolved_locale.clone(),
            };
            let (rows, total) = match plugin_storage::list_entries_for_host(
                db,
                &type_key,
                clamped_limit,
                clamped_offset,
                filters,
                eff_published,
                is_public_scope,
            )
            .await
            {
                Ok(result) => result,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let items: Vec<Value> = rows
                .into_iter()
                .map(|entry| {
                    project_entry_for_host(
                        owner_kind,
                        manifest,
                        action_caps,
                        actor_role,
                        is_public_scope,
                        &type_key,
                        &effective,
                        entry,
                    )
                })
                .collect();
            HostOperationResponse::ok_data(json!({ "items": items, "total": total }))
        }
        HostReadRequest::ContentTrashGet {
            type_key,
            entry_id,
            locale,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            match crate::storage::entry_lifecycle::get_trashed_entry_metadata_for_host(
                db,
                &type_key,
                entry_id,
                locale.as_deref(),
            )
            .await
            {
                Ok(entry) => HostOperationResponse::ok_data(json!({ "entry": entry })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::ContentTrashList {
            type_key,
            locale,
            limit,
            offset,
        } => {
            if !opendash_core::content::valid_content_type_key(&type_key) {
                return HostOperationResponse::err("Loại nội dung không hợp lệ.");
            }
            let clamped_limit = limit.unwrap_or(20).clamp(1, 50) as i64;
            let clamped_offset = offset.unwrap_or(0).min(10_000) as i64;
            match crate::storage::entry_lifecycle::list_trashed_entries_metadata_for_host(
                db,
                &type_key,
                locale.as_deref(),
                clamped_limit,
                clamped_offset,
            )
            .await
            {
                Ok((items, total)) => HostOperationResponse::ok_data(json!({
                    "items": items,
                    "total": total,
                    "limit": clamped_limit,
                    "offset": clamped_offset,
                })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::TaxonomyList {
            kind,
            locale,
            limit,
        } => {
            let def_opt = match taxonomy::get_definition(db, &kind).await {
                Ok(d) => d,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let Some(def) = def_opt else {
                return HostOperationResponse::ok_data(json!({ "items": [] }));
            };
            if def.provider_status != ProviderStatus::Active {
                return HostOperationResponse::ok_data(json!({ "items": [] }));
            }
            if !def.public && (is_public_scope || principal != ExecutionPrincipal::Admin) {
                return HostOperationResponse::err("permission_denied: taxonomy_private");
            }
            let clamped = limit.unwrap_or(50).clamp(1, 100) as i64;
            let page = match taxonomy::list_terms_for_host(
                db,
                Some(&kind),
                locale.as_deref(),
                clamped,
                0,
                is_public_scope,
            )
            .await
            {
                Ok(p) => p,
                Err(e) => return HostOperationResponse::err(e.1.to_string()),
            };
            let items: Vec<Value> = page
                .items
                .into_iter()
                .map(|t| {
                    json!({
                        "id": t.id,
                        "kind": t.kind,
                        "slug": t.slug,
                        "name": t.name,
                        "parent_id": t.parent_id,
                    })
                })
                .collect();
            HostOperationResponse::ok_data(json!({ "items": items }))
        }
        HostReadRequest::MediaGet { media_id } => {
            if !crate::storage::media::valid_media_id(&media_id) {
                return HostOperationResponse::ok_data(json!({ "media": Value::Null }));
            }
            if is_public_scope {
                match crate::storage::public_content::media_is_referenced(db, &media_id).await {
                    Ok(true) => {}
                    Ok(false) => {
                        return HostOperationResponse::ok_data(json!({ "media": Value::Null }));
                    }
                    Err(e) => return HostOperationResponse::err(e.1.to_string()),
                }
            }
            match media::find(db, &media_id).await {
                Ok(Some(m)) => {
                    let url = if is_public_scope {
                        format!("/media/{}/file", m.id)
                    } else {
                        format!("/api/admin/media/{}/file", m.id)
                    };
                    HostOperationResponse::ok_data(json!({
                        "media": {
                            "id": m.id,
                            "filename": m.filename,
                            "mime_type": m.mime_type,
                            "url": url,
                            "width": Some(m.width),
                            "height": Some(m.height),
                            "alt": Value::Null,
                        }
                    }))
                }
                Ok(None) => HostOperationResponse::ok_data(json!({ "media": Value::Null })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::SiteOptionsGet { locale } => {
            match settings::get_active_public_theme_options_for_locale(db, locale.as_deref()).await
            {
                Ok((_, map)) => HostOperationResponse::ok_data(json!({ "options": map })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::PluginSettingsGet => {
            let raw_map =
                match plugin_storage::read_raw_owner_option_map(db, owner_kind, plugin_id).await {
                    Ok(m) => m,
                    Err(e) => return HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
                };
            let mut settings_map = serde_json::Map::new();
            for field in &manifest.settings_schema {
                if field.is_ui_only() {
                    continue;
                }
                if field.field_type == FieldType::Secret {
                    let configured = raw_map
                        .get(&field.key)
                        .and_then(settings::is_secret_sentinel)
                        .is_some();
                    settings_map.insert(field.key.clone(), json!({ "configured": configured }));
                } else {
                    let val = raw_map
                        .get(&field.key)
                        .filter(|val| {
                            !val.as_object()
                                .is_some_and(|obj| obj.contains_key("secret_ref"))
                        })
                        .cloned()
                        .unwrap_or_else(|| default_value_for_field_spec(field));
                    settings_map.insert(field.key.clone(), val);
                }
            }
            HostOperationResponse::ok_data(json!({ "settings": settings_map }))
        }
        HostReadRequest::PluginSecretGet { key } => {
            if !manifest
                .settings_schema
                .iter()
                .any(|f| f.key == key && f.field_type == FieldType::Secret)
            {
                return HostOperationResponse::err(
                    "Khóa secret không tồn tại trong settings_schema.",
                );
            }
            let raw_map =
                match plugin_storage::read_raw_owner_option_map(db, owner_kind, plugin_id).await {
                    Ok(m) => m,
                    Err(e) => return HostOperationResponse::err(format!("Lỗi cơ sở dữ liệu: {e}")),
                };
            let Some(secret_ref) = raw_map.get(&key).and_then(settings::is_secret_sentinel) else {
                return HostOperationResponse::ok_data(json!({ "value": Value::Null }));
            };
            let Some(sec_root) = secrets_root else {
                return HostOperationResponse::err("SecretStore không khả dụng.");
            };
            let store = crate::secrets::SecretStore::for_plugin_dir(
                sec_root.join(owner_kind.secrets_subdir()).join(plugin_id),
            );
            match store.read_plugin_secret(secret_ref) {
                Ok(plaintext) => HostOperationResponse::ok_data(json!({ "value": plaintext })),
                Err(e) => HostOperationResponse::err(e.1.to_string()),
            }
        }
        HostReadRequest::L10nTranslate {
            text_domain,
            msgid,
            msgid_plural,
            count,
            context,
            locale,
        } => {
            let text = translate_for_host_read(
                manifest,
                text_domain.as_deref(),
                &msgid,
                msgid_plural.as_deref(),
                count,
                context.as_deref(),
                locale.as_deref(),
            );
            HostOperationResponse::ok_data(json!({ "text": text }))
        }
        HostReadRequest::L10nTranslateBatch { items } => {
            let texts: Vec<String> = items
                .iter()
                .map(|item| {
                    translate_for_host_read(
                        manifest,
                        item.text_domain.as_deref(),
                        &item.msgid,
                        item.msgid_plural.as_deref(),
                        item.count,
                        item.context.as_deref(),
                        item.locale.as_deref(),
                    )
                })
                .collect();
            HostOperationResponse::ok_data(json!({ "texts": texts }))
        }
    }
}

fn translate_for_host_read(
    manifest: &PluginManifest,
    text_domain: Option<&str>,
    msgid: &str,
    msgid_plural: Option<&str>,
    count: Option<i64>,
    context: Option<&str>,
    locale: Option<&str>,
) -> String {
    let domain = text_domain
        .or(manifest.text_domain.as_deref())
        .unwrap_or(&manifest.id);
    let loc = locale.unwrap_or("en").trim().to_ascii_lowercase();
    if domain == "opendash" && (loc == "vi" || loc.starts_with("vi-")) {
        static EMBEDDED: std::sync::OnceLock<opendash_core::l10n::Catalog> =
            std::sync::OnceLock::new();
        let cat = EMBEDDED.get_or_init(|| {
            opendash_core::l10n::parse_mo(crate::l10n::EMBEDDED_VI_MO).unwrap_or_default()
        });
        if let (Some(plural), Some(n)) = (msgid_plural, count) {
            if let Some(found) = cat.get_plural_with_context(context, msgid, n) {
                return found.to_owned();
            }
            return if n == 1 { msgid } else { plural }.to_owned();
        }
        if let Some(found) = cat.get_with_context(context, msgid) {
            return found.to_owned();
        }
    }
    if let (Some(plural), Some(n)) = (msgid_plural, count) {
        if n == 1 { msgid } else { plural }.to_owned()
    } else {
        msgid.to_owned()
    }
}
