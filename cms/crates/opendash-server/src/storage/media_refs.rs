//! Transactional media reachability & pin inventory (M8-S02-T01).

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};

const MAX_RETURNED_PINS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaPin {
    pub source_kind: &'static str,
    pub entity_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaPinSummary {
    pub total_pins: usize,
    pub pins: Vec<MediaPin>,
}

impl MediaPinSummary {
    pub fn is_referenced(&self) -> bool {
        self.total_pins > 0
    }
}

fn is_hex_byte(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

/// Checks whether `text` contains `media_id` as an exact 64-hex token,
/// `/media/<id>/file`, `/media/<id>/thumbnail`, or any boundary-delimited 64-hex match.
pub fn text_contains_media_pin(text: &str, media_id: &str) -> bool {
    if media_id.len() != 64 {
        return false;
    }
    let text_lower = text.to_ascii_lowercase();
    let id_lower = media_id.to_ascii_lowercase();
    let bytes = text_lower.as_bytes();
    let mut start = 0usize;
    while let Some(rel) = text_lower[start..].find(&id_lower) {
        let idx = start + rel;
        let end = idx + id_lower.len();
        let left_ok = idx == 0 || !is_hex_byte(bytes[idx - 1]);
        let right_ok = end == bytes.len() || !is_hex_byte(bytes[end]);
        if left_ok && right_ok {
            return true;
        }
        start = idx + 1;
    }
    false
}

pub fn json_contains_media_pin(value: &Value, media_id: &str) -> bool {
    match value {
        Value::String(s) => text_contains_media_pin(s, media_id),
        Value::Array(items) => items
            .iter()
            .any(|item| json_contains_media_pin(item, media_id)),
        Value::Object(map) => map.values().any(|v| json_contains_media_pin(v, media_id)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn parse_json_and_check_pin(raw: &str, media_id: &str, context: &str) -> ApiResult<bool> {
    let parsed: Value = serde_json::from_str(raw).map_err(|_| {
        ApiError::conflict(format!(
            "Dữ liệu JSON liên quan ({context}) bị lỗi định dạng; không thể xác minh tham chiếu media."
        ))
    })?;
    Ok(json_contains_media_pin(&parsed, media_id))
}

fn push_pin(summary: &mut MediaPinSummary, source_kind: &'static str, entity_id: String) {
    summary.total_pins += 1;
    if summary.pins.len() < MAX_RETURNED_PINS {
        summary.pins.push(MediaPin {
            source_kind,
            entity_id,
        });
    }
}

/// Collects all conservative pins to `media_id` inside an active SQLite transaction.
/// Fails closed (409) if any relevant persisted JSON is malformed/uninspectable.
pub async fn collect_pins_in_tx(
    conn: &mut SqliteConnection,
    media_id: &str,
) -> ApiResult<MediaPinSummary> {
    let mut summary = MediaPinSummary {
        total_pins: 0,
        pins: Vec::new(),
    };

    // 1. All content entries (draft, published, inactive, withdrawn)
    let entry_rows = sqlx::query("SELECT id, values_json FROM content_entries ORDER BY id ASC")
        .fetch_all(&mut *conn)
        .await?;
    let mut pinned_content_entries = HashSet::new();
    for row in entry_rows {
        let id: i64 = row.try_get("id")?;
        let values_json: String = row.try_get("values_json")?;
        if parse_json_and_check_pin(&values_json, media_id, "content_entries")? {
            pinned_content_entries.insert(id);
            push_pin(&mut summary, "content_entry", id.to_string());
        }
    }

    // 2. Site-owned typed template ASTs (all candidates, including inactive
    // themes and unpublished entry selectors). Preserve pins for every saved
    // override independently of public media delivery eligibility.
    let override_rows = sqlx::query("SELECT id, ast_json FROM template_overrides ORDER BY id ASC")
        .fetch_all(&mut *conn)
        .await?;
    let mut pinned_overrides = HashSet::new();
    for row in override_rows {
        let id: i64 = row.try_get("id")?;
        let ast_json: String = row.try_get("ast_json")?;
        if parse_json_and_check_pin(&ast_json, media_id, "template_overrides")? {
            push_pin(&mut summary, "template_override", id.to_string());
            pinned_overrides.insert(id);
        }
    }
    // Projection refs are derived from the AST transactionally, but still
    // count conservatively when an older projection is not represented by a
    // plain media token in the AST. Deduplicate against the AST scan.
    let projection_rows = sqlx::query(
        "SELECT DISTINCT override_id FROM template_override_media_refs
          WHERE media_id = ? ORDER BY override_id ASC",
    )
    .bind(media_id)
    .fetch_all(&mut *conn)
    .await?;
    for row in projection_rows {
        let id: i64 = row.try_get("override_id")?;
        if pinned_overrides.insert(id) {
            push_pin(&mut summary, "template_override", id.to_string());
        }
    }

    // Entry-owned Builder documents remain pinned even in editor mode or draft.
    // Deduplicate against `content_entries.values_json` pins above after unification.
    let documents: Vec<(i64, String)> = sqlx::query_as(
        "SELECT entry_id, document_json FROM entry_builder_documents ORDER BY entry_id",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut pinned_entries = HashSet::new();
    for (entry_id, raw) in documents {
        if parse_json_and_check_pin(&raw, media_id, "entry_builder_documents")? {
            pinned_entries.insert(entry_id);
            if !pinned_content_entries.contains(&entry_id) {
                push_pin(&mut summary, "entry_builder_document", entry_id.to_string());
            }
        }
    }
    let refs: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT entry_id FROM entry_builder_media_refs WHERE media_id = ? ORDER BY entry_id",
    ).bind(media_id).fetch_all(&mut *conn).await?;
    for entry_id in refs {
        if !pinned_content_entries.contains(&entry_id) && pinned_entries.insert(entry_id) {
            push_pin(&mut summary, "entry_builder_document", entry_id.to_string());
        }
    }
    // Owner selection ASTs are durable private metadata, never public grants.
    let selections: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, ast_json FROM owner_template_selections ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    for (id, raw) in selections {
        if parse_json_and_check_pin(&raw, media_id, "owner_template_selections")? {
            push_pin(&mut summary, "owner_template_selection", id.to_string());
        }
    }

    // 5. Owner option values + option media refs (even inactive/private themes and plugins)
    let option_rows =
        sqlx::query("SELECT owner_kind, owner_id, option_key, value_json FROM theme_option_values")
            .fetch_all(&mut *conn)
            .await?;
    let mut seen_owner_opts = std::collections::HashSet::new();
    for row in option_rows {
        let owner_kind: String = row.try_get("owner_kind")?;
        let owner_id: String = row.try_get("owner_id")?;
        let option_key: String = row.try_get("option_key")?;
        let value_json: String = row.try_get("value_json")?;
        let pin_kind = match owner_kind.as_str() {
            "plugin" => "plugin_option",
            "theme_functions" => "theme_functions_option",
            "options_page" => "options_page_option",
            _ => "theme_option",
        };
        if parse_json_and_check_pin(&value_json, media_id, "theme_option_values")? {
            let entity = format!("{owner_id}:{option_key}");
            seen_owner_opts.insert((owner_kind, entity.clone()));
            push_pin(&mut summary, pin_kind, entity);
        }
    }
    let opt_loc_rows = sqlx::query(
        "SELECT owner_kind, owner_id, locale, option_key, value_json FROM theme_option_locale_values",
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in opt_loc_rows {
        let owner_kind: String = row.try_get("owner_kind")?;
        let owner_id: String = row.try_get("owner_id")?;
        let locale: String = row.try_get("locale")?;
        let option_key: String = row.try_get("option_key")?;
        let value_json: String = row.try_get("value_json")?;
        if parse_json_and_check_pin(&value_json, media_id, "theme_option_locale_values")? {
            let pin_kind = if owner_kind == "options_page" {
                "options_page_option"
            } else {
                "theme_option"
            };
            let entity = format!("{owner_id}:{option_key}:{locale}");
            seen_owner_opts.insert((owner_kind, format!("{owner_id}:{option_key}")));
            push_pin(&mut summary, pin_kind, entity);
        }
    }
    let opt_ref_rows = sqlx::query(
        "SELECT DISTINCT owner_kind, owner_id, option_key FROM theme_option_media_refs WHERE media_id = ?",
    )
    .bind(media_id)
    .fetch_all(&mut *conn)
    .await?;
    for row in opt_ref_rows {
        let owner_kind: String = row.try_get("owner_kind")?;
        let owner_id: String = row.try_get("owner_id")?;
        let option_key: String = row.try_get("option_key")?;
        let pin_kind = match owner_kind.as_str() {
            "plugin" => "plugin_option",
            "theme_functions" => "theme_functions_option",
            "options_page" => "options_page_option",
            _ => "theme_option",
        };
        let entity = format!("{owner_id}:{option_key}");
        if !seen_owner_opts.contains(&(owner_kind, entity.clone())) {
            push_pin(&mut summary, pin_kind, entity);
        }
    }

    // 5b. Global site option values + global option media refs
    let global_rows = sqlx::query("SELECT option_key, value_json FROM site_global_option_values")
        .fetch_all(&mut *conn)
        .await?;
    let mut seen_global_opts = std::collections::HashSet::new();
    for row in global_rows {
        let option_key: String = row.try_get("option_key")?;
        let value_json: String = row.try_get("value_json")?;
        if parse_json_and_check_pin(&value_json, media_id, "site_global_option_values")? {
            seen_global_opts.insert(option_key.clone());
            push_pin(&mut summary, "site_global_option", option_key);
        }
    }
    let global_ref_rows =
        sqlx::query("SELECT option_key FROM site_global_option_media_refs WHERE media_id = ?")
            .bind(media_id)
            .fetch_all(&mut *conn)
            .await?;
    for row in global_ref_rows {
        let option_key: String = row.try_get("option_key")?;
        if !seen_global_opts.contains(&option_key) {
            push_pin(&mut summary, "site_global_option", option_key);
        }
    }

    // 6. Term meta values_json & term_field_media_refs
    let mut seen_term_meta = std::collections::HashSet::new();
    let term_meta_rows =
        sqlx::query("SELECT id, values_json FROM taxonomy_terms WHERE values_json != '{}'")
            .fetch_all(&mut *conn)
            .await?;
    for row in term_meta_rows {
        let term_id: i64 = row.try_get("id")?;
        let raw_json: String = row.try_get("values_json")?;
        if parse_json_and_check_pin(&raw_json, media_id, "taxonomy_terms.values_json")? {
            seen_term_meta.insert(term_id);
            push_pin(&mut summary, "term_meta", term_id.to_string());
        }
    }
    let term_media_ref_rows: Vec<i64> =
        sqlx::query_scalar("SELECT DISTINCT term_id FROM term_field_media_refs WHERE media_id = ?")
            .bind(media_id)
            .fetch_all(&mut *conn)
            .await?;
    for term_id in term_media_ref_rows {
        if seen_term_meta.insert(term_id) {
            push_pin(&mut summary, "term_meta", term_id.to_string());
        }
    }

    let entry_seo_rows = sqlx::query("SELECT entry_id FROM entry_seo WHERE og_image_id = ?")
        .bind(media_id)
        .fetch_all(&mut *conn)
        .await?;
    for row in entry_seo_rows {
        let entry_id: i64 = row.try_get("entry_id")?;
        push_pin(&mut summary, "entry_seo", entry_id.to_string());
    }
    let home_seo_rows: Vec<(String,)> =
        sqlx::query_as("SELECT locale FROM site_home_seo WHERE og_image_id = ?")
            .bind(media_id)
            .fetch_all(&mut *conn)
            .await?;
    let default_locale: Option<String> = if !home_seo_rows.is_empty() {
        sqlx::query_scalar(
            "SELECT code FROM site_locales WHERE is_default = 1 AND deleted_at IS NULL LIMIT 1",
        )
        .fetch_optional(&mut *conn)
        .await?
    } else {
        None
    };
    for (loc,) in home_seo_rows {
        let entity_id = if default_locale.as_deref() == Some(loc.as_str()) {
            "1".to_string()
        } else {
            loc
        };
        push_pin(&mut summary, "home_seo", entity_id);
    }

    // 7. Import runs (recovery journals / sample manifests)
    let import_rows = sqlx::query(
        "SELECT run_id, media_manifest_json, outcome_json FROM import_runs ORDER BY created_at ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in import_rows {
        let run_id: String = row.try_get("run_id")?;
        let manifest_json: String = row.try_get("media_manifest_json")?;
        let outcome_json: Option<String> = row.try_get("outcome_json")?;
        let mut pinned = parse_json_and_check_pin(&manifest_json, media_id, "import_runs")?;
        if !pinned {
            if let Some(outcome) = outcome_json {
                pinned = parse_json_and_check_pin(&outcome, media_id, "import_runs")?;
            }
        }
        if pinned {
            push_pin(&mut summary, "import_run", run_id);
        }
    }

    // 8. Opaque plugin/theme KV (even disabled/inactive extensions)
    let kv_rows = sqlx::query("SELECT owner_kind, plugin_id, key, value_json FROM plugin_kv")
        .fetch_all(&mut *conn)
        .await?;
    for row in kv_rows {
        let owner_kind: String = row.try_get("owner_kind")?;
        let plugin_id: String = row.try_get("plugin_id")?;
        let key: String = row.try_get("key")?;
        let value_json: String = row.try_get("value_json")?;
        if parse_json_and_check_pin(&value_json, media_id, "plugin_kv")? {
            let pin_kind = if owner_kind == "theme" {
                "theme_kv"
            } else {
                "plugin_kv"
            };
            push_pin(&mut summary, pin_kind, format!("{plugin_id}:{key}"));
        }
    }

    // 9. Taxonomy term featured images & term SEO og_image
    let term_rows: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM taxonomy_terms WHERE featured_image_media_id = ? ORDER BY id",
    )
    .bind(media_id)
    .fetch_all(&mut *conn)
    .await?;
    for term_id in term_rows {
        push_pin(&mut summary, "taxonomy_term", term_id.to_string());
    }
    let term_seo_rows: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM taxonomy_terms WHERE seo_og_image_id = ? ORDER BY id")
            .bind(media_id)
            .fetch_all(&mut *conn)
            .await?;
    for term_id in term_seo_rows {
        push_pin(&mut summary, "term_seo", term_id.to_string());
    }

    Ok(summary)
}

/// Ensures every referenced `media_id` exists and is `lifecycle_state = 'active'` inside the writer tx.
#[allow(dead_code)]
pub async fn ensure_media_active_in_tx(
    conn: &mut SqliteConnection,
    media_id: &str,
) -> ApiResult<i64> {
    let row = sqlx::query("SELECT lifecycle_state, generation FROM media_assets WHERE id = ?")
        .bind(media_id)
        .fetch_optional(&mut *conn)
        .await?;

    let Some(row) = row else {
        return Err(ApiError::bad_request(format!(
            "Media asset '{media_id}' không tồn tại."
        )));
    };
    let state: String = row.try_get("lifecycle_state")?;
    let generation: i64 = row.try_get("generation")?;
    if state != "active" {
        return Err(ApiError::conflict(format!(
            "Media asset '{media_id}' đang trong quá trình xóa; không thể tạo tham chiếu mới."
        )));
    }
    Ok(generation)
}

#[derive(Debug, Clone)]
pub struct MediaInventoryRow {
    pub id: String,
    pub kind: String,
    pub extension: String,
    pub lifecycle_state: String,
    #[allow(dead_code)]
    pub generation: i64,
    #[allow(dead_code)]
    pub version: i64,
}

fn extract_64hex_tokens(text: &str, out: &mut HashSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_hexdigit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            if i - start == 64 {
                out.insert(text[start..i].to_ascii_lowercase());
            }
        } else {
            i += 1;
        }
    }
}

pub async fn load_media_inventory_bounded(
    db: &SqlitePool,
    max_records: usize,
) -> ApiResult<Vec<MediaInventoryRow>> {
    let limit = (max_records + 1) as i64;
    let rows = sqlx::query(
        "SELECT id, kind, extension, lifecycle_state, generation, version
         FROM media_assets ORDER BY id ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;

    if rows.len() > max_records {
        return Err(ApiError::conflict(
            "Số bản ghi media vượt giới hạn reconcile (tối đa 10.000 bản ghi).",
        ));
    }

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(MediaInventoryRow {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            extension: row.try_get("extension")?,
            lifecycle_state: row.try_get("lifecycle_state")?,
            generation: row.try_get("generation")?,
            version: row.try_get("version")?,
        });
    }
    Ok(out)
}

pub async fn count_reconcile_records_bounded(
    db: &SqlitePool,
    max_records: usize,
) -> ApiResult<usize> {
    let media_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_assets")
        .fetch_one(db)
        .await?;
    let delete_job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_delete_jobs WHERE phase != 'finalized'")
            .fetch_one(db)
            .await?;
    let replace_job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM media_replace_jobs WHERE purged_at IS NULL")
            .fetch_one(db)
            .await?;
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contact_notification_attempts")
            .fetch_one(db)
            .await?;
    let total = media_count + delete_job_count + replace_job_count + attempt_count;
    if total as usize > max_records {
        return Err(ApiError::conflict(
            "Tổng số bản ghi cần đối soát vượt giới hạn reconcile (tối đa 10.000 bản ghi).",
        ));
    }
    Ok(total as usize)
}

pub async fn collect_referenced_secret_refs(db: &SqlitePool) -> ApiResult<HashSet<String>> {
    let mut refs = HashSet::new();

    let settings_rows =
        sqlx::query("SELECT secret_ref FROM site_smtp_settings WHERE secret_ref IS NOT NULL")
            .fetch_all(db)
            .await?;
    for row in settings_rows {
        let raw: String = row.try_get("secret_ref")?;
        extract_64hex_tokens(&raw, &mut refs);
    }

    let attempt_rows =
        sqlx::query("SELECT transport_snapshot_json FROM contact_notification_attempts")
            .fetch_all(db)
            .await?;
    for row in attempt_rows {
        let raw: String = row.try_get("transport_snapshot_json")?;
        extract_64hex_tokens(&raw, &mut refs);
    }

    Ok(refs)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtensionSecretInventoryItem {
    pub owner_subdir: String,
    pub package_id: String,
    pub secret_ref: String,
}

/// Collects extension secret files referenced in `theme_option_values` for plugins and
/// theme functions by parsing strictly typed JSON objects `{"secret_ref": "<64-hex>", ...}`
/// (never scanning raw text with `extract_64hex_tokens`, avoiding collisions with Media IDs).
pub async fn collect_referenced_extension_secret_refs(
    db: &SqlitePool,
) -> ApiResult<Vec<ExtensionSecretInventoryItem>> {
    let rows = sqlx::query(
        "SELECT owner_kind, owner_id, value_json
           FROM theme_option_values
          WHERE owner_kind IN ('plugin', 'theme_functions')
          ORDER BY owner_kind ASC, owner_id ASC, option_key ASC",
    )
    .fetch_all(db)
    .await?;

    let mut items = std::collections::BTreeSet::new();
    for row in rows {
        let owner_kind: String = row.try_get("owner_kind")?;
        let owner_id: String = row.try_get("owner_id")?;
        let value_json: String = row.try_get("value_json")?;
        if opendash_core::id::validate_package_id(&owner_id).is_err() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&value_json) else {
            continue;
        };
        let Some(secret_ref) = crate::storage::settings::is_secret_sentinel(&val) else {
            continue;
        };
        let owner_subdir = match owner_kind.as_str() {
            "plugin" => "plugins",
            "theme_functions" => "themes",
            _ => continue,
        };
        items.insert(ExtensionSecretInventoryItem {
            owner_subdir: owner_subdir.to_owned(),
            package_id: owner_id,
            secret_ref: secret_ref.to_owned(),
        });
    }

    Ok(items.into_iter().collect())
}

pub async fn is_media_unreferenced(db: &SqlitePool, media_id: &str) -> ApiResult<bool> {
    let mut conn = db.acquire().await?;
    let active_row: Option<(String,)> =
        sqlx::query_as("SELECT lifecycle_state FROM media_assets WHERE id = ?")
            .bind(media_id)
            .fetch_optional(&mut *conn)
            .await?;
    if active_row.is_some() {
        return Ok(false);
    }
    let pins = collect_pins_in_tx(&mut *conn, media_id).await?;
    Ok(!pins.is_referenced())
}
