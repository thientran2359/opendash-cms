//! Exact-entry and RouteContext Builder field resolution; never trust a client query
//! or bypass publication. Every read takes a consistent SQLite snapshot.
use opendash_core::field_schema::FieldType;
use std::collections::BTreeMap;

use opendash_core::{
    builder::{
        BindingSource, BindingTarget, BuilderDocument, ElementBinding, ElementContent, KnownElement,
    },
    content::{ContentTypeDefinition, PublicExposure},
};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::{
    error::ApiResult, public_context::CanonicalUrlBuilder, storage::media::valid_media_id,
};

#[derive(Debug, Clone)]
pub enum BindingResult {
    Value(String),
    Unavailable,
    Incompatible,
    UrlRejected,
}

pub type BindingResults = BTreeMap<String, BindingResult>;
type SourceRow = (String, String, String, Option<String>);

pub async fn resolve(
    db: &SqlitePool,
    document: &BuilderDocument,
    authenticated_preview: bool,
) -> ApiResult<BindingResults> {
    let mut tx = db.begin().await?;
    let results = resolve_in_tx(&mut tx, document, authenticated_preview).await?;
    tx.commit().await?;
    Ok(results)
}

pub async fn resolve_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    authenticated_preview: bool,
) -> ApiResult<BindingResults> {
    resolve_in_tx_with_route(tx, document, authenticated_preview, None).await
}

pub async fn resolve_in_tx_with_route(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    authenticated_preview: bool,
    route_entry: Option<(&str, &str)>,
) -> ApiResult<BindingResults> {
    resolve_in_tx_with_route_and_locale(tx, document, authenticated_preview, route_entry, None)
        .await
}

pub async fn resolve_in_tx_with_route_and_locale(
    tx: &mut Transaction<'_, Sqlite>,
    document: &BuilderDocument,
    authenticated_preview: bool,
    route_entry: Option<(&str, &str)>,
    render_locale: Option<&str>,
) -> ApiResult<BindingResults> {
    let active_locale = match render_locale {
        Some(raw) => {
            crate::storage::settings::resolve_requested_locale_in_conn(&mut **tx, Some(raw)).await?
        }
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut **tx).await?,
    };
    let mut cache: BTreeMap<(String, String, String), Option<SourceRow>> = BTreeMap::new();
    let mut results = BindingResults::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    let ElementContent::Known(element_type) = &element.content else {
                        continue;
                    };
                    let Some(binding) = element_type.binding() else {
                        continue;
                    };
                    let key = match &binding.source {
                        BindingSource::RouteContext { .. } => match route_entry {
                            Some((rt, rs)) => {
                                (rt.to_string(), rs.to_string(), active_locale.clone())
                            }
                            None => {
                                results.insert(element.id.clone(), BindingResult::Unavailable);
                                continue;
                            }
                        },
                        BindingSource::Title | BindingSource::Field { .. } => (
                            binding.type_key.clone(),
                            binding.entry_slug.clone(),
                            binding
                                .entry_locale
                                .clone()
                                .unwrap_or_else(|| active_locale.clone()),
                        ),
                    };
                    if !cache.contains_key(&key) {
                        // Defensive limit even for damaged/trusted stored JSON.
                        if cache.len() >= 32 {
                            results.insert(element.id.clone(), BindingResult::Unavailable);
                            continue;
                        }
                        // An entry's values NEVER leave storage on the public path until
                        // the published predicate and active provider check both hold.
                        let source_row: Option<(i64, String, String, String)> = sqlx::query_as(
                            "SELECT e.id, t.definition_json, e.title, e.values_json
                               FROM content_entries e
                               JOIN site_locales sl ON sl.code = e.locale AND sl.deleted_at IS NULL
                               JOIN content_types t ON t.key = e.content_type_key
                              WHERE e.content_type_key = ? AND e.slug = ? AND e.locale = ?
                                AND e.trashed_at IS NULL
                                AND t.provider_status = 'active'
                                AND (? = 1 OR (
                                    sl.enabled = 1
                                    AND (sl.is_default = 1 OR (SELECT multilingual_enabled FROM site_locales_state WHERE id = 1) = 1)
                                    AND e.publication_status = 'published'
                                    AND e.content_type_key != 'pattern'
                                    AND COALESCE(json_extract(t.definition_json, '$.settings.public'), 1) = 1
                                ))",
                        )
                        .bind(&key.0)
                        .bind(&key.1)
                        .bind(&key.2)
                        .bind(i64::from(authenticated_preview))
                        .fetch_optional(&mut **tx)
                        .await?;
                        let source = if let Some((entry_id, definition, title, values)) = source_row
                        {
                            // Preview may read draft fields, but an unpublished
                            // route must never become a public binding URL.
                            let url = if crate::storage::seo::resolve_public_entry_path_in_tx(
                                &mut **tx, entry_id,
                            )
                            .await?
                            .is_some()
                            {
                                crate::storage::permalinks::canonical_entry_path_in_tx(
                                    &mut **tx, entry_id,
                                )
                                .await?
                            } else {
                                None
                            };
                            Some((definition, title, values, url))
                        } else {
                            None
                        };
                        cache.insert(key.clone(), source);
                    }
                    let result = cache
                        .get(&key)
                        .and_then(|source| source.as_ref())
                        .map(|source| match &binding.source {
                            BindingSource::RouteContext { field_key } => extract_route_context(
                                binding.target,
                                field_key,
                                &key.0,
                                &key.1,
                                source,
                                matches!(element_type, KnownElement::Heading(_)),
                            ),
                            _ => extract(
                                binding,
                                source,
                                matches!(element_type, KnownElement::Heading(_)),
                            ),
                        })
                        .unwrap_or(BindingResult::Unavailable);
                    results.insert(element.id.clone(), result);
                }
            }
        }
    }
    Ok(results)
}

fn extract(
    binding: &ElementBinding,
    (definition, title, values, _custom_path): &SourceRow,
    heading: bool,
) -> BindingResult {
    let Ok(schema) = serde_json::from_str::<ContentTypeDefinition>(definition) else {
        return BindingResult::Incompatible;
    };
    if schema.key != binding.type_key || schema.validate().is_err() {
        return BindingResult::Incompatible;
    }
    let Ok(values) = serde_json::from_str::<BTreeMap<String, Value>>(values) else {
        return BindingResult::Incompatible;
    };
    let text = match &binding.source {
        BindingSource::Title => {
            if matches!(binding.target, BindingTarget::MediaId | BindingTarget::Url) {
                return BindingResult::Incompatible;
            }
            title.clone()
        }
        BindingSource::Field { key } => {
            match extract_field_value(&schema, &values, key, binding.target) {
                Ok(v) => v,
                Err(res) => return res,
            }
        }
        BindingSource::RouteContext { .. } => return BindingResult::Unavailable,
    };
    finalize_binding_value(binding.target, text, heading)
}

fn extract_route_context(
    target: BindingTarget,
    field_key: &str,
    type_key: &str,
    slug: &str,
    (definition, title, values, canonical_url): &SourceRow,
    heading: bool,
) -> BindingResult {
    let Ok(schema) = serde_json::from_str::<ContentTypeDefinition>(definition) else {
        return BindingResult::Incompatible;
    };
    if schema.key != type_key || schema.validate().is_err() {
        return BindingResult::Incompatible;
    }
    let Ok(values) = serde_json::from_str::<BTreeMap<String, Value>>(values) else {
        return BindingResult::Incompatible;
    };
    let text = match field_key {
        "title" => {
            if matches!(target, BindingTarget::MediaId | BindingTarget::Url) {
                return BindingResult::Incompatible;
            }
            title.clone()
        }
        "slug" => {
            if matches!(target, BindingTarget::MediaId | BindingTarget::Url) {
                return BindingResult::Incompatible;
            }
            slug.to_string()
        }
        "url" => {
            let Some(u) = canonical_url.clone() else {
                return BindingResult::Unavailable;
            };
            u
        }
        "cover_url" => {
            let Some(img_field) = schema.fields.iter().find(|f| {
                f.field_type == FieldType::Media && f.effective_acl().2 == PublicExposure::Public
            }) else {
                return BindingResult::Unavailable;
            };
            let Some(media_id) = values.get(&img_field.key).and_then(Value::as_str) else {
                return BindingResult::Unavailable;
            };
            if !valid_media_id(media_id) {
                return BindingResult::Incompatible;
            }
            match target {
                BindingTarget::MediaId => media_id.to_string(),
                BindingTarget::Url | BindingTarget::Text => format!("/media/{media_id}/file"),
                _ => return BindingResult::Incompatible,
            }
        }
        other => match extract_field_value(&schema, &values, other, target) {
            Ok(v) => v,
            Err(res) => return res,
        },
    };
    finalize_binding_value(target, text, heading)
}

fn extract_field_value(
    schema: &ContentTypeDefinition,
    values: &BTreeMap<String, Value>,
    key: &str,
    target: BindingTarget,
) -> Result<String, BindingResult> {
    let Some(field) = schema
        .fields
        .iter()
        .find(|field| field.key == key && field.effective_acl().2 == PublicExposure::Public)
    else {
        return Err(BindingResult::Unavailable);
    };
    let compatible = match target {
        BindingTarget::Text => matches!(
            field.field_type,
            FieldType::Text
                | FieldType::Textarea
                | FieldType::Richtext
                | FieldType::Number
                | FieldType::Boolean
                | FieldType::Date
                | FieldType::Select
        ),
        BindingTarget::MediaId => field.field_type == FieldType::Media,
        BindingTarget::Alt | BindingTarget::Label | BindingTarget::Url => {
            matches!(field.field_type, FieldType::Text | FieldType::Select)
        }
    };
    if !compatible {
        return Err(BindingResult::Incompatible);
    }
    let Some(value) = values.get(key).filter(|value| !value.is_null()) else {
        return Err(BindingResult::Unavailable);
    };
    match value {
        Value::String(s) => {
            if field.field_type == FieldType::Richtext
                && opendash_core::builder_blocks::contains_blocks(s)
            {
                Ok(opendash_core::builder_blocks::extract_visible_text(s))
            } else {
                Ok(s.clone())
            }
        }
        Value::Number(n) if target == BindingTarget::Text => Ok(n.to_string()),
        Value::Bool(v) if target == BindingTarget::Text => Ok(v.to_string()),
        _ => Err(BindingResult::Incompatible),
    }
}

fn finalize_binding_value(target: BindingTarget, text: String, heading: bool) -> BindingResult {
    // Bound data must meet the same output restrictions as static typed props;
    // rich_text is ALWAYS escaped later by the shared core renderer.
    let max = match target {
        BindingTarget::Text if heading => 512,
        BindingTarget::Text => 16_384,
        BindingTarget::MediaId => 128,
        BindingTarget::Alt => 512,
        BindingTarget::Label => 200,
        BindingTarget::Url => 2048,
    };
    if text.chars().count() > max
        || (matches!(
            target,
            BindingTarget::Text
                | BindingTarget::Label
                | BindingTarget::Url
                | BindingTarget::MediaId
        ) && text.trim().is_empty())
        || text
            .chars()
            .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
    {
        return BindingResult::Incompatible;
    }
    if target == BindingTarget::MediaId && !valid_media_id(&text) {
        return BindingResult::Incompatible;
    }
    if target == BindingTarget::Url && !opendash_core::builder::valid_button_url(&text) {
        return BindingResult::UrlRejected;
    }
    if target == BindingTarget::Url {
        return CanonicalUrlBuilder::normalize_internal_link(&text)
            .map(BindingResult::Value)
            .unwrap_or(BindingResult::UrlRejected);
    }
    BindingResult::Value(text)
}
