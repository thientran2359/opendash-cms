use std::collections::{BTreeMap, BTreeSet};

use opendash_core::builder::MAX_BUILDER_JSON_BYTES;
use opendash_core::builder_blocks::{
    MAX_BUILDER_BLOCK_BODY_BYTES, serialize_builder_document_to_blocks,
};
use opendash_core::content::{ContentMode, EffectiveContentSchema, FieldRole, PatchField};
use opendash_core::revisions::{CanonicalRevisionSeo, canonical_json_value};
use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use crate::error::{ApiError, ApiResult};

use super::capture::parse_content_mode;
use super::snapshot_queries::{LiveEntryState, sanitize_cleared_field_list};
use super::types::{AutosaveSurface, AutosaveUpsertInput, is_body_editable, is_field_editable};

pub(super) struct ExistingAutosaveRow {
    pub(super) source_surface: AutosaveSurface,
    pub(super) base_version: i64,
    pub(super) client_seq: i64,
    pub(super) title: String,
    pub(super) content_mode: ContentMode,
    pub(super) values: BTreeMap<String, Value>,
    pub(super) cleared_fields: Vec<String>,
    pub(super) builder_document_json: Option<String>,
    pub(super) term_ids: Vec<i64>,
    pub(super) primary_category_touched: bool,
    pub(super) primary_category_term_id: Option<i64>,
    pub(super) page_layout_touched: bool,
    pub(super) page_layout_key: Option<String>,
    pub(super) custom_template_touched: bool,
    pub(super) custom_template_key: Option<String>,
    pub(super) seo_touched: bool,
    pub(super) seo: Option<CanonicalRevisionSeo>,
    pub(super) parent_id_touched: bool,
    pub(super) parent_id: Option<i64>,
    pub(super) menu_order_touched: bool,
    pub(super) menu_order: Option<i32>,
    pub(super) updated_at: String,
}

pub(super) struct MergedAutosaveFields {
    pub(super) title: String,
    pub(super) content_mode: ContentMode,
    pub(super) values: BTreeMap<String, Value>,
    pub(super) cleared_fields: Vec<String>,
    pub(super) builder_document_json: Option<String>,
    pub(super) term_ids: Vec<i64>,
    pub(super) primary_category_touched: bool,
    pub(super) primary_category_term_id: Option<i64>,
    pub(super) page_layout_touched: bool,
    pub(super) page_layout_key: Option<String>,
    pub(super) custom_template_touched: bool,
    pub(super) custom_template_key: Option<String>,
    pub(super) seo_touched: bool,
    pub(super) seo: Option<CanonicalRevisionSeo>,
    pub(super) parent_id_touched: bool,
    pub(super) parent_id: Option<i64>,
    pub(super) menu_order_touched: bool,
    pub(super) menu_order: Option<i32>,
}

fn parse_surface(raw: &str) -> ApiResult<AutosaveSurface> {
    match raw {
        "editor" => Ok(AutosaveSurface::Editor),
        "studio" => Ok(AutosaveSurface::Studio),
        _ => Err(ApiError::internal("source_surface autosave không hợp lệ.")),
    }
}

pub(super) async fn load_existing_autosave_in_conn(
    conn: &mut SqliteConnection,
    entry_id: i64,
    user_id: i64,
    type_key: &str,
) -> ApiResult<Option<ExistingAutosaveRow>> {
    let row = sqlx::query(
        "SELECT source_surface, base_version, client_seq, title, content_mode, values_json,
                cleared_fields_json, builder_document_json, term_ids_json,
                primary_category_touched, primary_category_term_id,
                page_layout_touched, page_layout_key,
                custom_template_touched, custom_template_key,
                seo_touched, seo_json,
                parent_id_touched, parent_id,
                menu_order_touched, menu_order,
                updated_at
           FROM entry_autosaves
          WHERE entry_id = ? AND user_id = ? AND content_type_key = ?",
    )
    .bind(entry_id)
    .bind(user_id)
    .bind(type_key)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let parsed_values: BTreeMap<String, Value> =
        serde_json::from_str(&row.try_get::<String, _>("values_json")?)?;
    let cleared_fields: Vec<String> =
        serde_json::from_str(&row.try_get::<String, _>("cleared_fields_json")?)?;
    let term_ids: Vec<i64> = serde_json::from_str(&row.try_get::<String, _>("term_ids_json")?)?;
    let seo = row
        .try_get::<Option<String>, _>("seo_json")?
        .as_deref()
        .map(serde_json::from_str::<CanonicalRevisionSeo>)
        .transpose()?;

    Ok(Some(ExistingAutosaveRow {
        source_surface: parse_surface(&row.try_get::<String, _>("source_surface")?)?,
        base_version: row.try_get("base_version")?,
        client_seq: row.try_get("client_seq")?,
        title: row.try_get("title")?,
        content_mode: parse_content_mode(&row.try_get::<String, _>("content_mode")?)?,
        values: parsed_values,
        cleared_fields,
        builder_document_json: row.try_get("builder_document_json")?,
        term_ids,
        primary_category_touched: row.try_get::<i64, _>("primary_category_touched")? != 0,
        primary_category_term_id: row.try_get("primary_category_term_id")?,
        page_layout_touched: row.try_get::<i64, _>("page_layout_touched")? != 0,
        page_layout_key: row.try_get("page_layout_key")?,
        custom_template_touched: row.try_get::<i64, _>("custom_template_touched")? != 0,
        custom_template_key: row.try_get("custom_template_key")?,
        seo_touched: row.try_get::<i64, _>("seo_touched")? != 0,
        seo,
        parent_id_touched: row.try_get::<i64, _>("parent_id_touched")? != 0,
        parent_id: row.try_get("parent_id")?,
        menu_order_touched: row.try_get::<i64, _>("menu_order_touched")? != 0,
        menu_order: row.try_get("menu_order")?,
        updated_at: row.try_get("updated_at")?,
    }))
}

pub(super) fn validate_pattern_autosave_input(input: &AutosaveUpsertInput) -> ApiResult<()> {
    if input.source_surface != AutosaveSurface::Studio
        || input.values.as_ref().is_some_and(|v| !v.is_empty())
        || !input.cleared_fields.is_empty()
        || input.term_ids.as_ref().is_some_and(|t| !t.is_empty())
        || !matches!(input.primary_category_term_id, PatchField::Omitted)
        || !matches!(input.page_layout_key, PatchField::Omitted)
        || !matches!(input.custom_template_key, PatchField::Omitted)
        || !matches!(input.seo, PatchField::Omitted)
        || !matches!(input.parent_id, PatchField::Omitted)
        || !matches!(input.menu_order, PatchField::Omitted)
    {
        return Err(ApiError::bad_request(
            "Pattern autosave chỉ hỗ trợ source_surface = 'studio' và builder_document.",
        ));
    }
    Ok(())
}

pub(super) fn build_pattern_autosave_row(
    live: &LiveEntryState,
    existing: Option<&ExistingAutosaveRow>,
    input: &AutosaveUpsertInput,
) -> ApiResult<MergedAutosaveFields> {
    let Some(ref doc) = input.builder_document else {
        return Err(ApiError::bad_request(
            "Pattern autosave yêu cầu builder_document.",
        ));
    };
    let doc_bytes = doc
        .to_json()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if doc_bytes.len() > MAX_BUILDER_JSON_BYTES {
        return Err(ApiError::bad_request(
            "Builder document vượt giới hạn 256 KiB.",
        ));
    }
    let doc_raw = String::from_utf8(doc_bytes)
        .map_err(|_| ApiError::bad_request("Builder JSON không hợp lệ."))?;
    let title = match &input.title {
        PatchField::Set(t) => t.clone(),
        PatchField::Omitted => existing
            .map(|e| e.title.clone())
            .unwrap_or_else(|| live.payload.title.clone()),
        PatchField::Clear => unreachable!("validated at route layer"),
    };
    Ok(MergedAutosaveFields {
        title,
        content_mode: ContentMode::Builder,
        values: BTreeMap::new(),
        cleared_fields: Vec::new(),
        builder_document_json: Some(doc_raw),
        term_ids: Vec::new(),
        primary_category_touched: false,
        primary_category_term_id: None,
        page_layout_touched: false,
        page_layout_key: None,
        custom_template_touched: false,
        custom_template_key: None,
        seo_touched: false,
        seo: None,
        parent_id_touched: false,
        parent_id: None,
        menu_order_touched: false,
        menu_order: None,
    })
}

fn filter_editable_values_map(
    effective: &EffectiveContentSchema,
    source: &BTreeMap<String, Value>,
    caller_role: FieldRole,
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (k, v) in source {
        if effective
            .effective_fields
            .get(k)
            .is_some_and(|fe| is_field_editable(fe, caller_role))
            && !v.is_null()
        {
            out.insert(k.clone(), canonical_json_value(v));
        }
    }
    out
}

pub(super) fn build_studio_autosave_row(
    live: &LiveEntryState,
    existing: Option<&ExistingAutosaveRow>,
    input: &AutosaveUpsertInput,
    caller_role: FieldRole,
) -> ApiResult<MergedAutosaveFields> {
    let Some(ref doc) = input.builder_document else {
        return Err(ApiError::bad_request(
            "Studio autosave yêu cầu builder_document.",
        ));
    };
    let doc_bytes = doc
        .to_json()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if doc_bytes.len() > MAX_BUILDER_JSON_BYTES {
        return Err(ApiError::bad_request(
            "Builder document vượt giới hạn 256 KiB.",
        ));
    }
    let body_blocks = serialize_builder_document_to_blocks(doc)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if body_blocks.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
        return Err(ApiError::bad_request(
            "Body Studio vượt quá giới hạn 384 KiB.",
        ));
    }

    let mut values = match existing {
        Some(prev) => filter_editable_values_map(&live.effective, &prev.values, caller_role),
        None => filter_editable_values_map(&live.effective, &live.payload.values, caller_role),
    };
    values.insert("body".to_owned(), Value::String(body_blocks));

    let mut cleared = existing
        .map(|prev| {
            sanitize_cleared_field_list(
                &prev.cleared_fields,
                &live.effective,
                &live.content_type_key,
                caller_role,
            )
        })
        .unwrap_or_default();
    cleared.retain(|k| k != "body");

    let title = match &input.title {
        PatchField::Set(t) => t.clone(),
        PatchField::Omitted => existing
            .map(|e| e.title.clone())
            .unwrap_or_else(|| live.payload.title.clone()),
        PatchField::Clear => unreachable!(),
    };
    let mut term_ids = existing
        .map(|e| e.term_ids.clone())
        .unwrap_or_else(|| live.payload.term_ids.clone());
    term_ids.sort_unstable();
    term_ids.dedup();

    Ok(MergedAutosaveFields {
        title,
        content_mode: ContentMode::Builder,
        values,
        cleared_fields: cleared,
        builder_document_json: None,
        term_ids,
        primary_category_touched: existing.is_some_and(|e| e.primary_category_touched),
        primary_category_term_id: existing.and_then(|e| e.primary_category_term_id),
        page_layout_touched: existing.is_some_and(|e| e.page_layout_touched),
        page_layout_key: existing.and_then(|e| e.page_layout_key.clone()),
        custom_template_touched: existing.is_some_and(|e| e.custom_template_touched),
        custom_template_key: existing.and_then(|e| e.custom_template_key.clone()),
        seo_touched: existing.is_some_and(|e| e.seo_touched),
        seo: existing.and_then(|e| e.seo.clone()),
        parent_id_touched: existing.is_some_and(|e| e.parent_id_touched),
        parent_id: existing.and_then(|e| e.parent_id),
        menu_order_touched: existing.is_some_and(|e| e.menu_order_touched),
        menu_order: existing.and_then(|e| e.menu_order),
    })
}

pub(super) fn build_editor_autosave_row(
    live: &LiveEntryState,
    existing: Option<&ExistingAutosaveRow>,
    input: &AutosaveUpsertInput,
    caller_role: FieldRole,
) -> ApiResult<MergedAutosaveFields> {
    let body_editable = is_body_editable(&live.effective, &live.content_type_key, caller_role);
    let title = match &input.title {
        PatchField::Set(t) => t.clone(),
        PatchField::Omitted => existing
            .map(|e| e.title.clone())
            .unwrap_or_else(|| live.payload.title.clone()),
        PatchField::Clear => unreachable!(),
    };
    let content_mode = if body_editable {
        input.content_mode.unwrap_or(live.payload.content_mode)
    } else {
        live.payload.content_mode
    };
    let mut values = match &input.values {
        Some(v) => filter_editable_values_map(&live.effective, v, caller_role),
        None => match existing {
            Some(prev) => filter_editable_values_map(&live.effective, &prev.values, caller_role),
            None => filter_editable_values_map(&live.effective, &live.payload.values, caller_role),
        },
    };
    if !body_editable {
        values.remove("body");
    }

    let mut cleared = sanitize_cleared_field_list(
        &input.cleared_fields,
        &live.effective,
        &live.content_type_key,
        caller_role,
    );
    if !body_editable {
        cleared.retain(|k| k != "body");
    }
    let cleared_set: BTreeSet<&str> = cleared.iter().map(String::as_str).collect();
    values.retain(|k, _| !cleared_set.contains(k.as_str()));

    let mut term_ids = match &input.term_ids {
        Some(ids) => ids.clone(),
        None => existing
            .map(|e| e.term_ids.clone())
            .unwrap_or_else(|| live.payload.term_ids.clone()),
    };
    term_ids.sort_unstable();
    term_ids.dedup();

    let (primary_touched, primary_val) = match input.primary_category_term_id {
        PatchField::Omitted => (false, None),
        PatchField::Clear => (true, None),
        PatchField::Set(v) => (true, Some(v)),
    };
    let (layout_touched, layout_val) = resolve_opt_str_patch(&input.page_layout_key);
    let (template_touched, template_val) = resolve_opt_str_patch(&input.custom_template_key);
    let (seo_touched, seo_val) = match &input.seo {
        PatchField::Omitted => (false, None),
        PatchField::Clear => (true, None),
        PatchField::Set(s) => (
            true,
            CanonicalRevisionSeo {
                title: s.title.clone(),
                description: s.description.clone(),
                og_image_id: s.og_image_id.clone(),
                canonical_url: s.canonical_url.clone(),
                noindex: s.noindex,
            }
            .into_normalized(),
        ),
    };

    let (parent_id_touched, parent_id_val) = match input.parent_id {
        PatchField::Omitted => (false, None),
        PatchField::Clear => (true, None),
        PatchField::Set(v) => (true, Some(v)),
    };
    let (menu_order_touched, menu_order_val) = match input.menu_order {
        PatchField::Omitted => (false, None),
        PatchField::Clear => (true, Some(0)),
        PatchField::Set(v) => (true, Some(v)),
    };

    Ok(MergedAutosaveFields {
        title,
        content_mode,
        values,
        cleared_fields: cleared,
        builder_document_json: None,
        term_ids,
        primary_category_touched: primary_touched,
        primary_category_term_id: primary_val,
        page_layout_touched: layout_touched,
        page_layout_key: layout_val,
        custom_template_touched: template_touched,
        custom_template_key: template_val,
        seo_touched,
        seo: seo_val,
        parent_id_touched,
        parent_id: parent_id_val,
        menu_order_touched,
        menu_order: menu_order_val,
    })
}

fn resolve_opt_str_patch(patch: &PatchField<String>) -> (bool, Option<String>) {
    match patch {
        PatchField::Omitted => (false, None),
        PatchField::Clear => (true, None),
        PatchField::Set(v) => {
            let clean = v.trim();
            if clean.is_empty() {
                (true, None)
            } else {
                (true, Some(clean.to_owned()))
            }
        }
    }
}
