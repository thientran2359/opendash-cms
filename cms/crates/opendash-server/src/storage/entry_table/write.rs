use super::*;

impl Default for EntryTableFilter {
    fn default() -> Self {
        Self {
            q: String::new(),
            status: "all".into(),
            retained_trash: false,
            locale: None,
            category_id: None,
            tag_id: None,
            taxonomy_key: None,
            term_id: None,
            include_descendants: false,
            updated_month: None,
            sort: "updated_at".into(),
            order: "desc".into(),
            limit: 20,
            offset: 0,
        }
    }
}

pub(in crate::storage) fn push_entry_table_ctes(
    qb: &mut sqlx::QueryBuilder<Sqlite>,
    filter: &EntryTableFilter,
) {
    let need_cat_cte = filter.category_id.is_some() && filter.include_descendants;
    let need_gen_cte = filter.term_id.is_some() && filter.include_descendants;
    if !need_cat_cte && !need_gen_cte {
        return;
    }
    qb.push("WITH RECURSIVE ");
    if let Some(cat_id) = filter.category_id.filter(|_| need_cat_cte) {
        qb.push(
            "category_scope(id, depth) AS (\
             SELECT id, 1 FROM taxonomy_terms WHERE id = ",
        );
        qb.push_bind(cat_id);
        qb.push(
            " AND kind = 'category' \
             UNION ALL \
             SELECT c.id, category_scope.depth + 1 FROM taxonomy_terms c \
             JOIN category_scope ON c.parent_id = category_scope.id \
             WHERE category_scope.depth < 8)",
        );
        if need_gen_cte {
            qb.push(", ");
        }
    }
    if let Some(term_id) = filter.term_id.filter(|_| need_gen_cte) {
        qb.push(
            "generic_scope(id, depth) AS (\
             SELECT id, 1 FROM taxonomy_terms WHERE id = ",
        );
        qb.push_bind(term_id);
        qb.push(" AND kind = ");
        qb.push_bind(filter.taxonomy_key.clone());
        qb.push(
            " UNION ALL \
             SELECT c.id, generic_scope.depth + 1 FROM taxonomy_terms c \
             JOIN generic_scope ON c.parent_id = generic_scope.id \
             WHERE generic_scope.depth < 8)",
        );
    }
    qb.push(" ");
}

/// Appends identical static WHERE predicates for count and row queries, omitting
/// unused clauses so SQLite can satisfy common list/count queries via covering
/// index seeks on `idx_content_entries_lifecycle_list`.
pub(in crate::storage) fn push_entry_table_scope(
    qb: &mut sqlx::QueryBuilder<Sqlite>,
    type_key: &str,
    filter: &EntryTableFilter,
    resolved_locale: Option<&str>,
    active_locales: &[String],
) {
    qb.push(" FROM content_entries e WHERE e.content_type_key = ");
    qb.push_bind(type_key.to_owned());
    if let Some(loc) = resolved_locale {
        qb.push(" AND e.locale = ");
        qb.push_bind(loc.to_owned());
    } else if !active_locales.is_empty() {
        qb.push(" AND e.locale IN (");
        let mut sep = qb.separated(", ");
        for loc in active_locales {
            sep.push_bind(loc.clone());
        }
        qb.push(")");
    } else {
        qb.push(" AND 1 = 0");
    }
    if !filter.q.is_empty() {
        qb.push(" AND (instr(lower(e.title), lower(");
        qb.push_bind(filter.q.clone());
        qb.push(")) > 0 OR instr(lower(e.slug), lower(");
        qb.push_bind(filter.q.clone());
        qb.push(")) > 0)");
    }
    if let Some(month) = filter.updated_month.as_deref() {
        qb.push(" AND substr(e.updated_at, 1, 7) = ");
        qb.push_bind(month.to_owned());
    }
    if let Some(cat_id) = filter.category_id {
        if filter.include_descendants {
            qb.push(
                " AND EXISTS (SELECT 1 FROM entry_terms et \
                  WHERE et.entry_id = e.id AND et.term_id IN (SELECT id FROM category_scope))",
            );
        } else {
            qb.push(
                " AND EXISTS (SELECT 1 FROM entry_terms et \
                  WHERE et.entry_id = e.id AND et.term_id = ",
            );
            qb.push_bind(cat_id);
            qb.push(")");
        }
    }
    if let Some(tag_id) = filter.tag_id {
        qb.push(
            " AND EXISTS (SELECT 1 FROM entry_terms et \
              WHERE et.entry_id = e.id AND et.term_id = ",
        );
        qb.push_bind(tag_id);
        qb.push(")");
    }
    if let Some(term_id) = filter.term_id {
        if filter.include_descendants {
            qb.push(
                " AND EXISTS (SELECT 1 FROM entry_terms et \
                  WHERE et.entry_id = e.id AND et.term_id IN (SELECT id FROM generic_scope))",
            );
        } else {
            qb.push(
                " AND EXISTS (SELECT 1 FROM entry_terms et \
                  WHERE et.entry_id = e.id AND et.term_id = ",
            );
            qb.push_bind(term_id);
            qb.push(")");
        }
    }
}

pub(in crate::storage) async fn validate_terms(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    filter: &EntryTableFilter,
) -> ApiResult<()> {
    entries::validate_filter_in_tx(
        tx,
        type_key,
        &entries::EntryFilters {
            category_id: filter.category_id,
            tag_id: filter.tag_id,
            taxonomy_key: filter.taxonomy_key.clone(),
            term_id: filter.term_id,
            include_descendants: filter.include_descendants,
            locale: None,
        },
        false,
    )
    .await
}
