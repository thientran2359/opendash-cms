use std::collections::{BTreeMap, BTreeSet};

use opendash_core::{
    field_schema::{HydrationVisibility, default_value_for_field_spec},
    registry::DeclarationState,
};
use serde_json::Value;
use sqlx::SqliteConnection;

use super::get_options_page_in_conn;
use super::values::load_attached_options_groups_in_conn;
use crate::error::{ApiError, ApiResult};
use crate::storage::settings::{
    get_default_locale_code_in_conn, load_owner_option_maps_in_conn,
    resolve_requested_locale_in_conn,
};

pub(in crate::storage) async fn load_public_options_page_values_in_conn(
    conn: &mut SqliteConnection,
    page_ref: &str,
    requested_locale: Option<&str>,
    is_preview: bool,
    richtext_nonces: Option<&mut crate::template_sec::RichTextNonceTable>,
) -> ApiResult<BTreeMap<String, Value>> {
    if page_ref != "site_global" && !page_ref.starts_with("theme:") {
        let Some(page_key) = page_ref.strip_prefix("custom:") else {
            return Ok(BTreeMap::new());
        };
        let Some(page) = get_options_page_in_conn(conn, page_key).await? else {
            return Ok(BTreeMap::new());
        };
        if page.provider_status != opendash_core::registry::ProviderStatus::Active
            || page.declaration_state != DeclarationState::Declared
        {
            return Ok(BTreeMap::new());
        }
    }
    let default_locale = get_default_locale_code_in_conn(conn).await?;
    let target_locale = resolve_requested_locale_in_conn(conn, requested_locale).await?;
    let groups = load_attached_options_groups_in_conn(conn, page_ref).await?;
    let mut specs = Vec::new();
    for grp in groups {
        if grp.is_runtime_active() {
            specs.extend(grp.fields);
        }
    }
    if specs.is_empty() {
        return Ok(BTreeMap::new());
    }
    let (global_map, locale_map) = load_owner_option_maps_in_conn(
        conn,
        "options_page",
        page_ref,
        &default_locale,
        &target_locale,
    )
    .await?;
    let is_secondary = target_locale != default_locale;
    let mut raw = BTreeMap::new();
    for spec in &specs {
        if spec.is_ui_only() {
            continue;
        }
        let val = if is_secondary && spec.translatable {
            locale_map
                .get(&spec.key)
                .or_else(|| global_map.get(&spec.key))
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        } else {
            global_map
                .get(&spec.key)
                .cloned()
                .unwrap_or_else(|| default_value_for_field_spec(spec))
        };
        raw.insert(spec.key.clone(), val);
    }

    let mut media_ids = BTreeSet::new();
    let mut entry_ids = BTreeSet::new();
    let mut term_ids = BTreeSet::new();
    crate::field_hydrate::collect_hydration_ids(
        &specs,
        &raw,
        HydrationVisibility::OptionsPublicOnly,
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
        tag_media_provenance: true,
    };
    crate::field_hydrate::hydrate_field_values(
        &specs,
        &raw,
        &ctx,
        richtext_nonces,
        HydrationVisibility::OptionsPublicOnly,
    )
    .map_err(ApiError::internal)
}
