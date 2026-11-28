//! SITE-01 typed General Settings. SQL remains in storage.
use std::sync::LazyLock;

use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    field_schema::{
        CollectedMediaIds, FieldContext, FieldSpec, FieldType, collect_media_ids,
        default_value_for_field_spec, global_site_option_defaults, global_site_option_fields,
        valid_sha256_media_id, validate_field_values,
    },
    i18n::{
        CMS_CATALOG_VERSION, CMS_LANGUAGE_CATALOG, CmsLocaleOption, find_cms_locale,
        resolve_effective_ui_locale,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

use super::super::locales::{get_default_locale_code_in_conn, resolve_requested_locale_in_conn};
use super::schema::*;
use super::storage::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn resolve_merged_public_options_with_overrides_in_conn_with_nonces(
    conn: &mut SqliteConnection,
    active_theme: Option<(&str, &str)>,
    transient_global: Option<&BTreeMap<String, serde_json::Value>>,
    transient_theme: Option<&BTreeMap<String, serde_json::Value>>,
    is_preview: bool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    resolve_merged_public_options_with_all_overrides_in_conn_with_nonces(
        conn,
        active_theme,
        transient_global,
        transient_theme,
        None,
        is_preview,
        requested_locale,
        richtext_nonces,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) async fn resolve_merged_public_options_with_all_overrides_in_conn_with_nonces(
    conn: &mut SqliteConnection,
    active_theme: Option<(&str, &str)>,
    transient_global: Option<&BTreeMap<String, serde_json::Value>>,
    transient_theme: Option<&BTreeMap<String, serde_json::Value>>,
    transient_options_pages: Option<&BTreeMap<String, BTreeMap<String, serde_json::Value>>>,
    is_preview: bool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    resolve_merged_public_options_with_overrides_in_conn_with_options(
        conn,
        active_theme,
        transient_global,
        transient_theme,
        transient_options_pages,
        is_preview,
        requested_locale,
        richtext_nonces,
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn resolve_merged_public_options_with_overrides_in_conn_with_options(
    conn: &mut SqliteConnection,
    active_theme: Option<(&str, &str)>,
    transient_global: Option<&BTreeMap<String, serde_json::Value>>,
    transient_theme: Option<&BTreeMap<String, serde_json::Value>>,
    transient_options_pages: Option<&BTreeMap<String, BTreeMap<String, serde_json::Value>>>,
    is_preview: bool,
    requested_locale: Option<&str>,
    mut richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
    tag_media_provenance: bool,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;

    let global_fields = global_site_option_fields();
    let global_rows: Vec<(String, String)> =
        sqlx::query_as("SELECT option_key, value_json FROM site_global_option_values")
            .fetch_all(&mut *conn)
            .await?;
    let mut global_raw = global_site_option_defaults();
    for (k, v_json) in global_rows {
        if global_raw.contains_key(&k)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&v_json)
        {
            global_raw.insert(k, v);
        }
    }

    if let Some(g_ov) = transient_global {
        let normalized_g =
            validate_global_site_options_map(g_ov).map_err(ApiError::unprocessable)?;
        let g_media = collect_media_ids(&global_fields, &normalized_g);
        crate::storage::fields::verify_collected_media_ids_in_conn(conn, &g_media)
            .await
            .map_err(api_error_to_unprocessable)?;
        for (k, v) in normalized_g {
            global_raw.insert(k, v);
        }
    }

    let mut theme_specs = Vec::new();
    let mut theme_raw = BTreeMap::new();
    if let Some((theme_id, manifest_json)) = active_theme {
        theme_specs = parse_theme_options_schema(manifest_json).unwrap_or_default();
        if !theme_specs.is_empty() {
            let (saved_global_map, locale_map) =
                load_theme_option_maps_in_conn(conn, theme_id, &default_locale, &target_locale)
                    .await?;
            for spec in &theme_specs {
                if spec.is_ui_only() {
                    continue;
                }
                let maybe_val = if target_locale != default_locale && spec.translatable {
                    locale_map
                        .get(&spec.key)
                        .or_else(|| saved_global_map.get(&spec.key))
                        .cloned()
                } else {
                    saved_global_map.get(&spec.key).cloned()
                };
                if let Some(v) = maybe_val {
                    theme_raw.insert(spec.key.clone(), v);
                }
            }
        }

        if let Some(ov) = transient_theme {
            let spec_map: BTreeMap<&str, &FieldSpec> = theme_specs
                .iter()
                .filter(|s| !s.is_ui_only())
                .map(|s| (s.key.as_str(), s))
                .collect();
            let mut merged_for_validation = BTreeMap::new();
            for spec in &theme_specs {
                if spec.is_ui_only() {
                    continue;
                }
                let val = ov
                    .get(&spec.key)
                    .cloned()
                    .or_else(|| theme_raw.get(&spec.key).cloned())
                    .unwrap_or_else(|| default_value_for_field_spec(spec));
                merged_for_validation.insert(spec.key.clone(), val);
            }
            for (k, v) in ov {
                let Some(&spec) = spec_map.get(k.as_str()) else {
                    return Err(ApiError::unprocessable(
                        "Option key không tồn tại trong schema của theme.",
                    ));
                };
                validate_url_or_link_spec_strict(spec, v).map_err(ApiError::unprocessable)?;
            }
            validate_field_values(
                &theme_specs,
                FieldContext::Options,
                &merged_for_validation,
                Some(&theme_raw),
            )
            .map_err(ApiError::unprocessable)?;

            let ov_specs: Vec<FieldSpec> = ov
                .keys()
                .filter_map(|k| spec_map.get(k.as_str()).map(|s| (*s).clone()))
                .collect();
            let ov_media = collect_media_ids(&ov_specs, ov);
            crate::storage::fields::verify_collected_media_ids_in_conn(conn, &ov_media)
                .await
                .map_err(api_error_to_unprocessable)?;
            let req_loc = (target_locale != default_locale).then_some(target_locale.as_str());
            crate::storage::fields::validate_relational_refs_in_conn(
                conn,
                &theme_specs,
                &merged_for_validation,
                Some(&theme_raw),
                req_loc,
            )
            .await
            .map_err(api_error_to_unprocessable)?;

            for (k, v) in ov {
                theme_raw.insert(k.clone(), v.clone());
            }
        }
    } else if transient_theme.is_some_and(|m| !m.is_empty()) {
        return Err(ApiError::unprocessable(
            "Không có theme v1 đang kích hoạt để xem trước theme_options.",
        ));
    }

    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    crate::field_hydrate::collect_hydration_ids(
        &global_fields,
        &global_raw,
        crate::field_hydrate::HydrationVisibility::OptionsPublicOnly,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );
    crate::field_hydrate::collect_hydration_ids(
        &theme_specs,
        &theme_raw,
        crate::field_hydrate::HydrationVisibility::OptionsPublicOnly,
        &mut media_ids,
        &mut entry_ids,
        &mut term_ids,
    );

    let entries = crate::storage::fields::load_entry_ref_hydration_map_in_conn(
        conn,
        &entry_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    let terms = crate::storage::fields::load_term_ref_hydration_map_in_conn(
        conn,
        &term_ids,
        is_preview,
        Some(&target_locale),
    )
    .await?;
    for term in terms.values() {
        if let Some(ref mid) = term.featured_image_media_id {
            media_ids.insert(mid.clone());
        }
    }
    let media = crate::storage::fields::load_media_hydration_map_in_conn(conn, &media_ids).await?;

    let ctx = crate::field_hydrate::HydrationContext {
        media: &media,
        entries: &entries,
        terms: &terms,
        is_preview,
        tag_media_provenance,
    };
    let mut site_options = crate::field_hydrate::hydrate_field_values(
        &global_fields,
        &global_raw,
        &ctx,
        richtext_nonces.as_deref_mut(),
        crate::field_hydrate::HydrationVisibility::OptionsPublicOnly,
    )
    .map_err(ApiError::internal)?;

    let has_field_groups =
        crate::storage::content::table_exists_in_tx(conn, "content_field_groups").await?;
    let transient_global_extra = transient_options_pages.and_then(|m| m.get("site_global"));
    if has_field_groups {
        let global_reserved: BTreeSet<String> = global_fields
            .iter()
            .filter(|s| !s.is_ui_only())
            .map(|s| s.key.clone())
            .collect();
        let extra_global =
            crate::storage::options_pages::load_public_options_page_values_with_override_in_conn(
                conn,
                "site_global",
                requested_locale,
                is_preview,
                transient_global_extra,
                Some(&global_reserved),
                richtext_nonces.as_deref_mut(),
            )
            .await?;
        for (k, v) in extra_global {
            site_options.entry(k).or_insert(v);
        }
    } else if transient_global_extra.is_some_and(|m| !m.is_empty()) {
        return Err(ApiError::unprocessable(
            "Không có field group công khai nào gắn với site_global để xem trước.",
        ));
    }

    let mut merged = site_options.clone();
    if !theme_specs.is_empty() {
        let theme_hydrated = crate::field_hydrate::hydrate_field_values(
            &theme_specs,
            &theme_raw,
            &ctx,
            richtext_nonces.as_deref_mut(),
            crate::field_hydrate::HydrationVisibility::OptionsPublicOnly,
        )
        .map_err(ApiError::internal)?;
        merged.extend(theme_hydrated);
    }
    if let Some((theme_id, _)) = active_theme {
        let page_ref = format!("theme:{theme_id}");
        let transient_theme_extra = transient_options_pages.and_then(|m| m.get(&page_ref));
        if has_field_groups {
            let theme_reserved: BTreeSet<String> = theme_specs
                .iter()
                .filter(|s| !s.is_ui_only())
                .map(|s| s.key.clone())
                .collect();
            let extra_theme =
                crate::storage::options_pages::load_public_options_page_values_with_override_in_conn(
                    conn,
                    &page_ref,
                    requested_locale,
                    is_preview,
                    transient_theme_extra,
                    Some(&theme_reserved),
                    richtext_nonces,
                )
                .await?;
            for (k, v) in extra_theme {
                merged.entry(k).or_insert(v);
            }
        } else if transient_theme_extra.is_some_and(|m| !m.is_empty()) {
            return Err(ApiError::unprocessable(
                "Không có field group công khai nào gắn với theme để xem trước.",
            ));
        }
    }

    Ok((site_options, merged))
}

pub async fn get_active_merged_public_options_in_conn_for_locale_with_nonces(
    conn: &mut SqliteConnection,
    is_preview: bool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let active: Option<(String, String)> = if let Some(dev) =
        crate::dev_mode::DevOverlayConfig::global()
        && let Ok(Some(snap)) = dev.load_dev_theme()
    {
        Some((snap.manifest.theme_id, snap.manifest_json))
    } else {
        sqlx::query_as(
            "SELECT r.theme_id, r.manifest_json
             FROM theme_active_selection AS a
             JOIN theme_revisions AS r ON r.revision_id = a.revision_id
             WHERE a.id = 1 AND r.source_format = 'v1'",
        )
        .fetch_optional(&mut *conn)
        .await?
    };
    let active_ref = active.as_ref().map(|(id, m)| (id.as_str(), m.as_str()));
    resolve_merged_public_options_with_overrides_in_conn_with_nonces(
        conn,
        active_ref,
        None,
        None,
        is_preview,
        requested_locale,
        richtext_nonces,
    )
    .await
}

pub async fn get_active_merged_public_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    is_preview: bool,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_merged_public_options_in_conn_for_locale_with_nonces(
        conn,
        is_preview,
        requested_locale,
        None,
    )
    .await
}

pub async fn get_active_merged_public_options_in_conn(
    conn: &mut SqliteConnection,
    is_preview: bool,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_merged_public_options_in_conn_for_locale(conn, is_preview, None).await
}

/// Returns `(site_options, merged_options)` where `merged_options` overlays
/// active theme `public: true` options on top of durable Global public options.
pub async fn get_active_public_theme_options_for_locale_with_nonces(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let mut conn = db.acquire().await?;
    get_active_merged_public_options_in_conn_for_locale_with_nonces(
        &mut conn,
        false,
        requested_locale,
        richtext_nonces,
    )
    .await
}

pub async fn get_active_public_theme_options_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    let mut conn = db.acquire().await?;
    let active = if let Some(dev) = crate::dev_mode::DevOverlayConfig::global()
        && let Ok(Some(snap)) = dev.load_dev_theme()
    {
        Some((snap.manifest.theme_id, snap.manifest_json))
    } else {
        sqlx::query_as::<_, (String, String)>(
            "SELECT r.theme_id, r.manifest_json
             FROM theme_active_selection AS a
             JOIN theme_revisions AS r ON r.revision_id = a.revision_id
             WHERE a.id = 1 AND r.source_format = 'v1'",
        )
        .fetch_optional(&mut *conn)
        .await?
    };
    let active_ref = active.as_ref().map(|(id, m)| (id.as_str(), m.as_str()));
    resolve_merged_public_options_with_overrides_in_conn_with_options(
        &mut conn,
        active_ref,
        None,
        None,
        None,
        false,
        requested_locale,
        None,
        false,
    )
    .await
}

pub async fn get_active_public_theme_options(
    db: &SqlitePool,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    get_active_public_theme_options_for_locale(db, None).await
}

/// Resolve merged global + public theme values against the revision already captured by the caller.
pub(in crate::storage) async fn get_public_theme_options_in_conn_for_locale_with_nonces(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    requested_locale: Option<&str>,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    resolve_merged_public_options_with_overrides_in_conn_with_options(
        conn,
        Some((theme_id, manifest_json)),
        None,
        None,
        None,
        false,
        requested_locale,
        richtext_nonces,
        true,
    )
    .await
}

pub(in crate::storage) async fn get_public_theme_options_in_conn_for_locale(
    conn: &mut SqliteConnection,
    theme_id: &str,
    manifest_json: &str,
    requested_locale: Option<&str>,
) -> ApiResult<(
    BTreeMap<String, serde_json::Value>,
    BTreeMap<String, serde_json::Value>,
)> {
    resolve_merged_public_options_with_overrides_in_conn_with_options(
        conn,
        Some((theme_id, manifest_json)),
        None,
        None,
        None,
        false,
        requested_locale,
        None,
        false,
    )
    .await
}
