use super::*;

fn collect_active_media_refs_rec(
    nodes: &[opendash_core::builder::BlockNode],
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
    refs: &mut DocumentMediaRefs,
) {
    for node in nodes {
        let Some(entry) = schemas.get(&node.block_id) else {
            continue;
        };
        if entry.status != ProviderStatus::Active
            || entry.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        let mut collected = entry.descriptor.schema.collect_media_ids(&node.props);
        if node.block_id == "core/video"
            && let Some(poster_id) = node
                .props
                .get("poster")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
        {
            collected.image_ids.remove(poster_id);
            refs.poster_image_ids.insert(poster_id.to_owned());
        }
        refs.image_ids.extend(collected.image_ids);
        refs.video_ids.extend(collected.video_ids);
        refs.any_media_ids.extend(collected.any_ids);
        refs.file_ids.extend(collected.file_ids);
        collect_active_media_refs_rec(&node.children, schemas, refs);
    }
}

pub(crate) fn extract_document_media_refs(
    document: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> DocumentMediaRefs {
    let mut refs = DocumentMediaRefs::default();
    collect_active_media_refs_rec(&document.blocks, schemas, &mut refs);
    refs
}

/// Resolve media-kind 'any' & 'file' against the same SQLite snapshot as the write.
/// The returned sets are validated, active assets and safe to project into ACL.
pub(crate) async fn resolve_document_media_refs_in_conn(
    conn: &mut sqlx::SqliteConnection,
    document: &BuilderDocument,
) -> ApiResult<DocumentMediaRefs> {
    let schemas = crate::storage::page::load_unified_block_catalog_in_conn(conn).await?;
    let mut refs = extract_document_media_refs(document, &schemas);
    for (expected_kind, ids) in [
        ("image", &refs.image_ids),
        ("image", &refs.poster_image_ids),
        ("video", &refs.video_ids),
    ] {
        for id in ids {
            if !valid_media_id(id) {
                return Err(ApiError::bad_request("Invalid Builder media ID"));
            }
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = ? AND kind = ? AND lifecycle_state = 'active')",
            )
            .bind(id)
            .bind(expected_kind)
            .fetch_one(&mut *conn)
            .await?;
            if exists == 0 {
                return Err(ApiError::bad_request(
                    "Builder media không tồn tại hoặc không đúng loại trong Media Library.",
                ));
            }
        }
    }
    for id in std::mem::take(&mut refs.any_media_ids)
        .into_iter()
        .chain(std::mem::take(&mut refs.file_ids))
    {
        if !valid_media_id(&id) {
            return Err(ApiError::bad_request("Invalid Builder media ID"));
        }
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM media_assets WHERE id = ? AND lifecycle_state = 'active'",
        )
        .bind(&id)
        .fetch_optional(&mut *conn)
        .await?;
        match kind.as_deref() {
            Some("image") => {
                refs.image_ids.insert(id);
            }
            Some("video") => {
                refs.video_ids.insert(id);
            }
            _ => {
                return Err(ApiError::bad_request(
                    "Builder media không tồn tại hoặc không đúng loại trong Media Library.",
                ));
            }
        }
    }
    Ok(refs)
}

/// A Pattern is a private, single-hop Builder document. References are stored
/// separately so delete guards and public media ACL never parse client JSON.
pub fn pattern_ids(document: &BuilderDocument) -> BTreeSet<i64> {
    let mut ids = BTreeSet::new();
    document.walk_blocks(|node, _| {
        if node.block_id == "core/pattern-ref"
            && let Some(pid) = parse_positive_i64_prop(node.props.get("pattern_id"))
        {
            ids.insert(pid);
        }
    });
    ids
}

fn collect_active_pattern_refs_rec(
    nodes: &[opendash_core::builder::BlockNode],
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
    ids: &mut BTreeSet<i64>,
    instances: &mut Vec<(String, i64)>,
) {
    for node in nodes {
        let Some(entry) = schemas.get(&node.block_id) else {
            continue;
        };
        if entry.status != ProviderStatus::Active
            || entry.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        if node.block_id == "core/pattern-ref"
            && let Some(pid) = parse_positive_i64_prop(node.props.get("pattern_id"))
        {
            ids.insert(pid);
            instances.push((node.id.clone(), pid));
        }
        ids.extend(entry.descriptor.schema.collect_pattern_ids(&node.props));
        collect_active_pattern_refs_rec(&node.children, schemas, ids, instances);
    }
}

pub fn pattern_ids_with_schemas(
    document: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> BTreeSet<i64> {
    let mut ids = BTreeSet::new();
    let mut instances = Vec::new();
    collect_active_pattern_refs_rec(&document.blocks, schemas, &mut ids, &mut instances);
    ids
}

pub(in crate::storage) fn pattern_ref_instances_with_schemas(
    document: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> Vec<(String, i64)> {
    let mut ids = BTreeSet::new();
    let mut instances = Vec::new();
    collect_active_pattern_refs_rec(&document.blocks, schemas, &mut ids, &mut instances);
    instances
}

pub(in crate::storage) fn node_count(document: &BuilderDocument) -> usize {
    let mut count = 0usize;
    document.walk_blocks(|_, _| count += 1);
    count
}

/// One batched query inside the caller's read/write transaction, maximum 16
/// distinct IDs. Invalid or missing Pattern data is not rendered publicly.
pub(in crate::storage) async fn load_pattern_rows_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &BTreeSet<i64>,
) -> ApiResult<Vec<(i64, String, Option<String>)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    if ids.len() > 16 || ids.iter().any(|id| *id <= 0) {
        return Err(ApiError::bad_request(
            "Builder exceeds 16 Pattern references.",
        ));
    }
    let mut sql = QueryBuilder::<Sqlite>::new(
        "SELECT e.id, e.slug, d.document_json FROM content_entries e \
         JOIN content_types t ON t.key = e.content_type_key \
         LEFT JOIN entry_builder_documents d ON d.entry_id = e.id \
         WHERE e.content_type_key = 'pattern' AND t.owner_kind = 'core' \
           AND t.provider_status = 'active' AND t.declaration_state = 'declared' \
           AND e.id IN (",
    );
    {
        let mut list = sql.separated(", ");
        for id in ids {
            list.push_bind(id);
        }
        list.push_unseparated(")");
    }
    Ok(sql.build_query_as().fetch_all(&mut **tx).await?)
}

pub async fn validate_media_refs_in_tx(
    conn: &mut sqlx::SqliteConnection,
    document: &BuilderDocument,
) -> ApiResult<()> {
    resolve_document_media_refs_in_conn(conn, document).await?;
    Ok(())
}

pub async fn check_preview_images(db: &SqlitePool, document: &BuilderDocument) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    validate_media_refs_in_tx(&mut conn, document).await
}

pub(in crate::storage) async fn replace_entry_builder_refs(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    document: Option<&BuilderDocument>,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM entry_pattern_refs WHERE source_entry_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "DELETE FROM entry_builder_media_refs WHERE entry_id = ? AND source_kind = 'builder'",
    )
    .bind(id)
    .execute(&mut **tx)
    .await?;
    if let Some(document) = document {
        let schemas = crate::storage::page::load_unified_block_catalog_in_conn(&mut **tx).await?;
        for pattern_id in pattern_ids_with_schemas(document, &schemas) {
            // The target may later be force-deleted; the FK cascade and
            // warning-only renderer safely preserve that historical AST.
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM content_entries WHERE id=? AND content_type_key='pattern')",
            )
            .bind(pattern_id)
            .fetch_one(&mut **tx)
            .await?;
            if exists != 0 {
                sqlx::query(
                    "INSERT INTO entry_pattern_refs(source_entry_id,target_pattern_id) VALUES(?,?)",
                )
                .bind(id)
                .bind(pattern_id)
                .execute(&mut **tx)
                .await?;
            }
        }
        let refs = resolve_document_media_refs_in_conn(&mut **tx, document).await?;
        for (role, ids) in [
            ("image", refs.image_ids),
            ("video", refs.video_ids),
            ("poster", refs.poster_image_ids),
        ] {
            for media_id in ids {
                sqlx::query("INSERT INTO entry_builder_media_refs(entry_id, media_id, ref_role) VALUES (?, ?, ?)")
                    .bind(id)
                    .bind(media_id)
                    .bind(role)
                    .execute(&mut **tx)
                    .await?;
            }
        }
    }
    Ok(())
}
