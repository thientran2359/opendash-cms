use super::*;

pub async fn list_dynamic_options(
    db: &SqlitePool,
    themes_root: &Path,
    source: SelectOptionsSource,
    type_key: Option<&str>,
    locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<Vec<DynamicSelectOption>> {
    let mut conn = db.acquire().await?;
    list_dynamic_options_in_conn(&mut conn, themes_root, source, type_key, locale, actor_role).await
}

pub async fn list_dynamic_options_in_conn(
    conn: &mut SqliteConnection,
    themes_root: &Path,
    source: SelectOptionsSource,
    type_key: Option<&str>,
    locale: Option<&str>,
    actor_role: FieldRole,
) -> ApiResult<Vec<DynamicSelectOption>> {
    match source {
        SelectOptionsSource::Menus => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT menu_key, title FROM site_menus ORDER BY title ASC, menu_key ASC LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(value, label)| DynamicSelectOption {
                    value,
                    label,
                    meta: None,
                })
                .collect())
        }
        SelectOptionsSource::MenuSlots => {
            let Some(active) = load_active_theme_v3_in_conn(conn, themes_root).await? else {
                return Ok(Vec::new());
            };
            let labels = collect_menu_slot_labels(active.manifest.menus.as_ref());
            let Ok(manifest_json) = serde_json::to_string(&active.manifest) else {
                return Ok(Vec::new());
            };
            let slots = extract_declared_menu_slots(&manifest_json);
            Ok(slots
                .into_iter()
                .take(MAX_DYNAMIC_OPTIONS)
                .map(|slot_key| {
                    let label = labels
                        .get(&slot_key)
                        .cloned()
                        .unwrap_or_else(|| slot_key.clone());
                    DynamicSelectOption {
                        value: slot_key,
                        label,
                        meta: None,
                    }
                })
                .collect())
        }
        SelectOptionsSource::ContactForms => {
            let rows: Vec<(i64, String, String)> = sqlx::query_as(
                "SELECT id, slug, title
                   FROM contact_forms
                  WHERE status = 'active'
                  ORDER BY title ASC, id ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(id, slug, title)| DynamicSelectOption {
                    value: id.to_string(),
                    label: title,
                    meta: Some(slug),
                })
                .collect())
        }
        SelectOptionsSource::ContentTypes => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT key, definition_json
                   FROM content_types
                  WHERE provider_status = 'active' AND key != 'pattern'
                  ORDER BY key ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            let mut items: Vec<DynamicSelectOption> = rows
                .into_iter()
                .map(|(key, raw_def)| {
                    let label = serde_json::from_str::<Value>(&raw_def)
                        .ok()
                        .and_then(|v| v.get("label").and_then(Value::as_str).map(str::to_owned))
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| key.clone());
                    DynamicSelectOption {
                        value: key,
                        label,
                        meta: None,
                    }
                })
                .collect();
            items.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.value.cmp(&b.value)));
            Ok(items)
        }
        SelectOptionsSource::Taxonomies => {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT key, label
                   FROM taxonomy_definitions
                  WHERE provider_status = 'active' AND declaration_state = 'declared'
                  ORDER BY label ASC, key ASC
                  LIMIT ?",
            )
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(value, label)| DynamicSelectOption {
                    value,
                    label,
                    meta: None,
                })
                .collect())
        }
        SelectOptionsSource::ContextFields => {
            let mut items = vec![
                DynamicSelectOption {
                    value: "title".to_owned(),
                    label: "Tiêu đề".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "slug".to_owned(),
                    label: "Đường dẫn (Slug)".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "url".to_owned(),
                    label: "URL công khai".to_owned(),
                    meta: Some("core".to_owned()),
                },
                DynamicSelectOption {
                    value: "cover_url".to_owned(),
                    label: "Ảnh đại diện (Cover)".to_owned(),
                    meta: Some("core".to_owned()),
                },
            ];
            if let Some(tk) = type_key.filter(|k| !k.is_empty() && *k != "pattern") {
                if !valid_snake_key(tk) {
                    return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
                }
                if let Some(schema) = load_effective_content_schema_in_tx(conn, tk).await?
                    && schema.base.status == ProviderStatus::Active
                    && schema.base.descriptor.settings.public
                {
                    let core_keys: BTreeSet<&str> = ["title", "slug", "url", "cover_url"].into();
                    for (key, entry) in &schema.effective_fields {
                        if items.len() >= MAX_DYNAMIC_OPTIONS {
                            break;
                        }
                        if core_keys.contains(key.as_str()) {
                            continue;
                        }
                        if entry.effective_status == EffectiveFieldStatus::Active
                            && entry.definition.effective_acl().2 == PublicExposure::Public
                            && entry.definition.effective_acl().0 <= actor_role
                        {
                            items.push(DynamicSelectOption {
                                value: key.clone(),
                                label: entry.definition.label.clone(),
                                meta: Some(format!("field:{}", tk)),
                            });
                        }
                    }
                }
            }
            Ok(items)
        }
        SelectOptionsSource::TemplateParts => {
            let Some(active) = load_active_theme_v3_in_conn(conn, themes_root).await? else {
                return Ok(Vec::new());
            };
            Ok(active
                .manifest
                .parts
                .into_iter()
                .take(MAX_DYNAMIC_OPTIONS)
                .map(|(part_key, file)| DynamicSelectOption {
                    value: part_key.clone(),
                    label: part_key,
                    meta: Some(file),
                })
                .collect())
        }
        SelectOptionsSource::Patterns => {
            let clean_locale = locale.filter(|l| !l.trim().is_empty());
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
                "SELECT e.id, e.title, e.slug, e.locale
                   FROM content_entries e
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                  WHERE e.content_type_key = 'pattern'
                    AND e.publication_status = 'draft'
                    AND e.trashed_at IS NULL
                    AND (?1 IS NULL OR e.locale = ?1)
                  ORDER BY e.id DESC
                  LIMIT ?2",
            )
            .bind(clean_locale)
            .bind(MAX_DYNAMIC_OPTIONS as i64)
            .fetch_all(&mut *conn)
            .await?;
            Ok(rows
                .into_iter()
                .map(|(id, title, _slug, loc)| DynamicSelectOption {
                    value: id.to_string(),
                    label: title,
                    meta: Some(loc),
                })
                .collect())
        }
    }
}

pub async fn resolve_entries_by_ids(
    db: &SqlitePool,
    ids: &[i64],
) -> ApiResult<Vec<ResolvedEntrySummary>> {
    let mut conn = db.acquire().await?;
    resolve_entries_by_ids_in_conn(&mut conn, ids).await
}

pub async fn resolve_entries_by_ids_in_conn(
    conn: &mut SqliteConnection,
    ids: &[i64],
) -> ApiResult<Vec<ResolvedEntrySummary>> {
    if ids.is_empty() || ids.len() > MAX_ENTRY_RESOLVE_IDS || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let raw_ids = serde_json::to_string(ids)?;
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.title, e.slug, e.publication_status, e.locale
           FROM content_entries e
           JOIN content_types ct ON ct.key = e.content_type_key
           JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
           JOIN json_each(?1) req ON CAST(req.value AS INTEGER) = e.id
          WHERE e.trashed_at IS NULL
            AND ct.provider_status = 'active'
            AND e.content_type_key != 'pattern'
          ORDER BY e.id ASC",
    )
    .bind(raw_ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, content_type_key, title, slug, publication_status, locale)| {
                ResolvedEntrySummary {
                    id,
                    content_type_key,
                    title,
                    slug,
                    publication_status,
                    locale,
                }
            },
        )
        .collect())
}

pub async fn load_media_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    media_ids: &BTreeSet<String>,
) -> ApiResult<BTreeMap<String, HydratedMediaRow>> {
    let ids: Vec<&str> = media_ids
        .iter()
        .filter(|id| valid_sha256_media_id(id))
        .take(MAX_HYDRATE_MEDIA_IDS)
        .map(String::as_str)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(String, String, String, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
            "SELECT id, kind, mime_type, version, width, height
               FROM media_assets
              WHERE lifecycle_state = 'active'
                AND id IN (SELECT value FROM json_each(?1))",
        )
        .bind(&raw_json)
        .fetch_all(&mut *conn)
        .await?;
        for (id, kind, mime_type, version, width, height) in rows {
            out.insert(
                id.clone(),
                HydratedMediaRow {
                    id,
                    kind,
                    mime_type,
                    version: version.max(1),
                    width,
                    height,
                    variants: Vec::new(),
                },
            );
        }
        let variant_rows: Vec<(String, String, String, i64, i64, i64)> = sqlx::query_as(
            "SELECT v.media_id, v.spec, v.format, v.width, v.height, v.byte_size
               FROM media_variants v
               JOIN media_assets a
                 ON a.id = v.media_id AND a.version = v.version
              WHERE a.lifecycle_state = 'active'
                AND v.media_id IN (SELECT value FROM json_each(?1))
              ORDER BY v.width ASC, v.format ASC",
        )
        .bind(&raw_json)
        .fetch_all(&mut *conn)
        .await?;
        for (media_id, spec_raw, format_raw, vw, vh, byte_size) in variant_rows {
            let (Some(spec), Some(format)) = (
                opendash_core::media::MediaVariantSpec::parse(&spec_raw),
                opendash_core::media::MediaVariantFormat::parse_db(&format_raw),
            ) else {
                continue;
            };
            if let Some(entry) = out.get_mut(&media_id) {
                entry.variants.push(HydratedMediaVariantRow {
                    spec,
                    format,
                    width: u32::try_from(vw.max(0)).unwrap_or(0),
                    height: u32::try_from(vh.max(0)).unwrap_or(0),
                    byte_size: u64::try_from(byte_size.max(0)).unwrap_or(0),
                });
            }
        }
    }
    Ok(out)
}

pub async fn load_entry_ref_hydration_map_in_conn(
    conn: &mut SqliteConnection,
    entry_ids: &BTreeSet<i64>,
    is_preview: bool,
    _active_locale: Option<&str>,
) -> ApiResult<BTreeMap<i64, HydratedEntryRefRow>> {
    let ids: Vec<i64> = entry_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .take(MAX_HYDRATE_ENTRY_IDS)
        .collect();
    let mut out = BTreeMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let preview_flag: i64 = i64::from(is_preview);
    let mut raw_rows: Vec<(
        i64,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = Vec::new();
    for chunk in ids.chunks(HYDRATE_SQL_CHUNK) {
        let raw_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT e.id, e.content_type_key, e.title, e.slug, e.values_json, e.published_at, c.path
                   FROM content_entries e
                   JOIN content_types t ON t.key = e.content_type_key
                   JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                   LEFT JOIN permalink_route_claims c
                     ON c.target_kind = 'entry'
                    AND c.target_id = CAST(e.id AS TEXT)
                    AND c.locale = e.locale
                  WHERE e.id IN (SELECT value FROM json_each(?1))
                    AND e.trashed_at IS NULL
                    AND t.provider_status = 'active'
                    AND e.content_type_key NOT IN ('pattern', 'section')
                    AND (?2 = 1 OR (
                        e.publication_status = 'published'
                        AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                        AND sl.enabled = 1
                        AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                    ))",
            )
            .bind(raw_json)
            .bind(preview_flag)
            .fetch_all(&mut *conn)
            .await?;
        raw_rows.extend(rows);
    }

    let distinct_types: BTreeSet<String> = raw_rows
        .iter()
        .map(|(_, type_key, ..)| type_key.clone())
        .collect();
    let mut cover_field_by_type: BTreeMap<String, Option<String>> = BTreeMap::new();
    for type_key in distinct_types {
        let cover_key = load_effective_content_schema_in_tx(conn, &type_key)
            .await?
            .and_then(|schema| {
                schema.effective_fields.into_iter().find_map(|(k, fe)| {
                    if fe.effective_status == EffectiveFieldStatus::Active
                        && fe.definition.effective_acl().2 == PublicExposure::Public
                        && fe.definition.effective_acl().0 == FieldRole::Editor
                        && fe.definition.field_type == opendash_core::field_schema::FieldType::Media
                    {
                        Some(k)
                    } else {
                        None
                    }
                })
            });
        cover_field_by_type.insert(type_key, cover_key);
    }

    let mut cover_media_ids = BTreeSet::new();
    let mut parsed_rows = Vec::with_capacity(raw_rows.len());
    for (id, type_key, title, slug, values_json, published_at, claim_path) in raw_rows {
        let values =
            serde_json::from_str::<BTreeMap<String, Value>>(&values_json).unwrap_or_default();
        let candidate_cover_id = cover_field_by_type
            .get(&type_key)
            .and_then(|opt| opt.as_deref())
            .and_then(|field_key| values.get(field_key))
            .and_then(Value::as_str)
            .filter(|mid| valid_sha256_media_id(mid))
            .map(str::to_owned);
        if let Some(ref mid) = candidate_cover_id {
            cover_media_ids.insert(mid.clone());
        }
        let summary_plain = values
            .get("summary")
            .or_else(|| values.get("body"))
            .and_then(Value::as_str)
            .map(crate::content_render::strip_richtext_to_plain)
            .unwrap_or_default();
        let excerpt = truncate_entry_excerpt(&summary_plain);
        let url = match claim_path {
            Some(path) => path,
            None if is_preview => {
                format!("/api/admin/content-types/{type_key}/entries/{id}/preview")
            }
            None => format!("/{slug}"),
        };
        parsed_rows.push((
            id,
            type_key,
            title,
            slug,
            url,
            candidate_cover_id,
            excerpt,
            published_at,
        ));
    }

    let cover_media_map = load_media_hydration_map_in_conn(conn, &cover_media_ids).await?;
    let media_prefix = if is_preview {
        "/api/admin/media"
    } else {
        "/media"
    };
    for (id, type_key, title, slug, url, candidate_cover_id, excerpt, published_at) in parsed_rows {
        let cover_url = candidate_cover_id.and_then(|mid| {
            cover_media_map
                .get(&mid)
                .filter(|m| m.kind == "image")
                .map(|m| format!("{media_prefix}/{}/file?v={}", m.id, m.version.max(1)))
        });
        out.insert(
            id,
            HydratedEntryRefRow {
                id,
                type_key,
                title,
                slug,
                url,
                cover_url,
                excerpt,
                published_at,
            },
        );
    }
    Ok(out)
}
