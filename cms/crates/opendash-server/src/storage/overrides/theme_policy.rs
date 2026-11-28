use super::*;

use opendash_core::content::{
    EffectiveTermSchema, FieldGroupDomain, LocationEvalContext, evaluate_location_rules,
    is_restricted_to_admin_only,
};

/// Only the manifest captured for this request and active DB declarations can
/// authorize theme URL expressions; no caller-provided preview values contribute permissions.
pub(crate) fn active_theme_url_policy(
    active: Option<&ActiveThemeV3>,
) -> ApiResult<template_sec::ThemeUrlPolicy> {
    match active {
        Some(active) => settings::theme_url_policy(&serde_json::to_string(&active.manifest)?)
            .map_err(ApiError::internal),
        None => Ok(settings::global_theme_url_policy()),
    }
}

pub(crate) fn active_theme_url_policy_for_target(
    active: Option<&ActiveThemeV3>,
    target_key: Option<&str>,
    db_schema: Option<&EffectiveContentSchema>,
) -> ApiResult<template_sec::ThemeUrlPolicy> {
    let mut policy = active_theme_url_policy(active)?;
    let (Some(active), Some(tkey)) = (active, target_key) else {
        return Ok(policy);
    };
    let manifest_json = serde_json::to_string(&active.manifest)?;
    let decls =
        crate::theme::parse_theme_declarations(&manifest_json).map_err(ApiError::internal)?;
    let (ex_site, ex_theme) = crate::theme::declared_extra_site_and_theme_options_fields(
        &decls,
        &active.manifest.theme_id,
    );
    policy = policy
        .with_custom_options_pages(&crate::theme::declared_custom_options_pages_fields(&decls))
        .with_extra_site_and_theme_options(&ex_site, &ex_theme);

    if let Some(cpt) = crate::theme::cpt_scope_for_target_key(&active.manifest, tkey) {
        let mut by_key = BTreeMap::new();
        for spec in crate::theme::declared_fields_for_cpt(&decls, cpt) {
            by_key.insert(spec.key.clone(), spec);
        }
        if let Some(eff) = db_schema
            && eff.base.descriptor.key == cpt
        {
            for (k, fe) in &eff.effective_fields {
                if fe.effective_status == opendash_core::content::EffectiveFieldStatus::Active {
                    by_key.insert(k.clone(), fe.definition.clone());
                } else {
                    by_key.remove(k);
                }
            }
        }
        let merged: Vec<_> = by_key.into_values().collect();
        policy = policy.with_cpt_entry_fields(&merged);
    } else if crate::template_sec::is_taxonomy_target_name(tkey) {
        let tax_scope = crate::theme::taxonomy_scope_for_target_key(tkey);
        let term_fields = crate::theme::declared_fields_for_taxonomy(&decls, tax_scope);
        policy = policy.with_taxonomy_term_fields(&term_fields);
    }
    Ok(policy)
}

pub(crate) async fn active_theme_url_policy_in_conn(
    conn: &mut SqliteConnection,
    active: Option<&ActiveThemeV3>,
    target_key: Option<&str>,
    db_cpt_schema: Option<&EffectiveContentSchema>,
    db_term_schema: Option<&EffectiveTermSchema>,
) -> ApiResult<template_sec::ThemeUrlPolicy> {
    let mut policy = active_theme_url_policy_for_target(active, target_key, db_cpt_schema)?;

    if crate::storage::content::table_exists_in_tx(conn, "custom_options_pages").await?
        && crate::storage::content::table_exists_in_tx(conn, "content_field_groups").await?
    {
        let page_keys: Vec<String> = sqlx::query_scalar(
            "SELECT page_key FROM custom_options_pages
              WHERE provider_status = 'active' AND declaration_state = 'declared'
              ORDER BY page_key ASC",
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut pages_map: BTreeMap<String, Vec<opendash_core::field_schema::FieldSpec>> =
            BTreeMap::new();
        if let Some(act) = active {
            let manifest_json = serde_json::to_string(&act.manifest)?;
            if let Ok(decls) = crate::theme::parse_theme_declarations(&manifest_json) {
                pages_map.extend(crate::theme::declared_custom_options_pages_fields(&decls));
            }
        }
        for page_key in page_keys {
            let page_ref = format!("custom:{page_key}");
            let groups = crate::storage::content::load_attached_groups_in_tx(
                conn,
                "options_page",
                &page_ref,
                FieldGroupDomain::OptionsPage,
            )
            .await?;
            let eval_ctx = LocationEvalContext {
                options_page: Some(page_ref),
                user_role: FieldRole::Editor,
                ..Default::default()
            };
            let list = pages_map.entry(page_key).or_default();
            let mut seen: BTreeSet<String> = list.iter().map(|s| s.key.clone()).collect();
            for grp in groups {
                if grp.is_runtime_active()
                    && !is_restricted_to_admin_only(&grp.location)
                    && evaluate_location_rules(&grp.location, &eval_ctx)
                {
                    for spec in grp.fields {
                        if seen.insert(spec.key.clone()) {
                            list.push(spec);
                        }
                    }
                }
            }
        }
        policy = policy.with_custom_options_pages(&pages_map);

        let extra_site = load_active_options_ref_specs_in_conn(conn, "site_global").await?;
        let extra_theme = if let Some(act) = active {
            let tref = format!("theme:{}", act.manifest.theme_id);
            load_active_options_ref_specs_in_conn(conn, &tref).await?
        } else {
            Vec::new()
        };
        policy = policy.with_extra_site_and_theme_options(&extra_site, &extra_theme);
    }

    let resolved_term_schema = match db_term_schema {
        Some(eff) => Some(eff.clone()),
        None => {
            if let Some(tkey) = target_key
                && let Some(tax_key) = crate::theme::taxonomy_scope_for_target_key(tkey)
            {
                crate::storage::taxonomy::load_effective_term_schema_in_tx(conn, tax_key).await?
            } else {
                None
            }
        }
    };
    if let Some(eff) = resolved_term_schema {
        let mut by_key: BTreeMap<String, opendash_core::field_schema::FieldSpec> = BTreeMap::new();
        if let Some(act) = active {
            let manifest_json = serde_json::to_string(&act.manifest)?;
            if let Ok(decls) = crate::theme::parse_theme_declarations(&manifest_json) {
                for spec in
                    crate::theme::declared_fields_for_taxonomy(&decls, Some(&eff.taxonomy.key))
                {
                    by_key.insert(spec.key.clone(), spec);
                }
            }
        }
        let eval_ctx = LocationEvalContext {
            taxonomy_term: Some(eff.taxonomy.key.clone()),
            user_role: FieldRole::Editor,
            ..Default::default()
        };
        for grp in &eff.attached_groups {
            if grp.is_runtime_active()
                && !is_restricted_to_admin_only(&grp.location)
                && evaluate_location_rules(&grp.location, &eval_ctx)
            {
                for spec in &grp.fields {
                    if eff.effective_fields.get(&spec.key).is_some_and(|fe| {
                        fe.effective_status == opendash_core::content::EffectiveFieldStatus::Active
                    }) {
                        by_key.insert(spec.key.clone(), spec.clone());
                    }
                }
            }
        }
        let fields: Vec<_> = by_key.into_values().collect();
        policy = policy.with_taxonomy_term_fields(&fields);
    } else if target_key.is_some_and(crate::template_sec::is_taxonomy_target_name)
        && crate::storage::content::table_exists_in_tx(conn, "content_field_groups").await?
    {
        let mut by_key: BTreeMap<String, opendash_core::field_schema::FieldSpec> = BTreeMap::new();
        if let Some(act) = active {
            let manifest_json = serde_json::to_string(&act.manifest)?;
            if let Ok(decls) = crate::theme::parse_theme_declarations(&manifest_json) {
                for spec in crate::theme::declared_fields_for_taxonomy(&decls, None) {
                    by_key.insert(spec.key.clone(), spec);
                }
            }
        }
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT group_id FROM content_field_groups
              WHERE domain = 'taxonomy_term' AND status = 'active' AND active = 1
              ORDER BY group_id ASC",
        )
        .fetch_all(&mut *conn)
        .await?;
        for gid in ids {
            if let Some(grp) = crate::storage::content::load_field_group_in_tx(conn, &gid).await?
                && grp.active
                && grp.status == opendash_core::content::FieldGroupStatus::Active
                && !is_restricted_to_admin_only(&grp.location)
            {
                for spec in grp.fields {
                    by_key.insert(spec.key.clone(), spec);
                }
            }
        }
        let fields: Vec<_> = by_key.into_values().collect();
        policy = policy.with_taxonomy_term_fields(&fields);
    }

    Ok(policy)
}

async fn load_active_options_ref_specs_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
) -> ApiResult<Vec<opendash_core::field_schema::FieldSpec>> {
    let groups = crate::storage::content::load_attached_groups_in_tx(
        conn,
        "options_page",
        page_ref,
        FieldGroupDomain::OptionsPage,
    )
    .await?;
    let eval_ctx = LocationEvalContext {
        options_page: Some(page_ref.to_owned()),
        user_role: FieldRole::Editor,
        ..Default::default()
    };
    let mut out = Vec::new();
    for grp in groups {
        if grp.is_runtime_active()
            && !is_restricted_to_admin_only(&grp.location)
            && evaluate_location_rules(&grp.location, &eval_ctx)
        {
            out.extend(grp.fields);
        }
    }
    Ok(out)
}

pub(crate) async fn active_theme_url_policy_for_route(
    db: &SqlitePool,
    active: Option<&ActiveThemeV3>,
    target_key: Option<&str>,
    db_cpt_schema: Option<&EffectiveContentSchema>,
    db_term_schema: Option<&EffectiveTermSchema>,
) -> ApiResult<template_sec::ThemeUrlPolicy> {
    let mut conn = db.acquire().await?;
    active_theme_url_policy_in_conn(&mut conn, active, target_key, db_cpt_schema, db_term_schema)
        .await
}
