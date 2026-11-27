use super::*;

pub(in crate::storage) async fn batch_load_entry_parent_views_in_conn(
    conn: &mut sqlx::SqliteConnection,
    parent_ids: &[i64],
) -> ApiResult<
    std::collections::BTreeMap<i64, (String, String, crate::content_render::EntryParentView)>,
> {
    let mut out = std::collections::BTreeMap::new();
    let unique: Vec<i64> = parent_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if unique.is_empty() {
        return Ok(out);
    }
    for chunk in unique.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT p.id, p.content_type_key, p.locale, p.title, p.slug, c.path
               FROM content_entries p
               JOIN site_locales sl ON sl.code = p.locale AND sl.enabled = 1 AND sl.deleted_at IS NULL
                AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
               JOIN content_types t ON t.key = p.content_type_key
               LEFT JOIN permalink_route_claims c
                 ON c.target_kind = 'entry' AND c.target_id = CAST(p.id AS TEXT) AND c.locale = p.locale
              WHERE p.id IN (SELECT value FROM json_each(?))
                AND p.trashed_at IS NULL
                AND p.publication_status = 'published'
                AND p.content_type_key NOT IN ('pattern', 'section')
                AND t.provider_status = 'active'
                AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (id, type_key, locale, title, slug, url) in rows {
            out.insert(
                id,
                (
                    type_key,
                    locale,
                    crate::content_render::EntryParentView {
                        id,
                        title,
                        slug,
                        url,
                    },
                ),
            );
        }
    }
    Ok(out)
}

pub(in crate::storage) async fn batch_load_entry_eval_contexts_in_conn(
    conn: &mut sqlx::SqliteConnection,
    entry_meta: &[(i64, String, Option<String>, Option<String>)],
    user_role: opendash_core::content::FieldRole,
) -> ApiResult<std::collections::BTreeMap<i64, opendash_core::content::LocationEvalContext>> {
    let mut out = std::collections::BTreeMap::new();
    if entry_meta.is_empty() {
        return Ok(out);
    }
    let reading_rows: Vec<(Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT homepage_page_id, posts_page_id FROM site_reading_settings")
            .fetch_all(&mut *conn)
            .await?;
    let mut home_ids = std::collections::BTreeSet::new();
    let mut posts_ids = std::collections::BTreeSet::new();
    for (hp, pp) in reading_rows {
        if let Some(id) = hp {
            home_ids.insert(id);
        }
        if let Some(id) = pp {
            posts_ids.insert(id);
        }
    }
    let ids: Vec<i64> = entry_meta.iter().map(|(id, ..)| *id).collect();
    let mut terms_by_entry: std::collections::BTreeMap<
        i64,
        std::collections::BTreeSet<(String, i64)>,
    > = std::collections::BTreeMap::new();
    for chunk in ids.chunks(500) {
        let ids_json = serde_json::to_string(chunk)?;
        let rows: Vec<(i64, String, i64)> = sqlx::query_as(
            "SELECT et.entry_id, t.kind, et.term_id
               FROM entry_terms et
               JOIN taxonomy_terms t ON t.id = et.term_id
              WHERE et.entry_id IN (SELECT value FROM json_each(?))",
        )
        .bind(ids_json)
        .fetch_all(&mut *conn)
        .await?;
        for (eid, kind, tid) in rows {
            terms_by_entry.entry(eid).or_default().insert((kind, tid));
        }
    }
    for (id, type_key, page_layout, custom_template) in entry_meta {
        let special_page = if home_ids.contains(id) {
            Some("homepage".to_owned())
        } else if posts_ids.contains(id) {
            Some("posts_page".to_owned())
        } else {
            None
        };
        out.insert(
            *id,
            opendash_core::content::LocationEvalContext {
                content_type: Some(type_key.clone()),
                page_layout: page_layout.clone(),
                custom_template: custom_template.clone(),
                special_page,
                entry_term_pairs: terms_by_entry.remove(id).unwrap_or_default(),
                entry_id: Some(*id),
                user_role,
                ..Default::default()
            },
        );
    }
    Ok(out)
}
