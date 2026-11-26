use super::*;

pub(crate) const ACTIVE_ENTRY_UNALIASED: &str = "trashed_at IS NULL";

/// Server-only committed entry mutation kind for trusted post-commit hooks and receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EntryMutationKind {
    Save,
    Metadata,
    Publish,
    Unpublish,
    Trash,
    Restore,
    Purge,
}

/// Server-only commit receipt produced strictly by storage inside the committed writer transaction.
/// Deliberately does NOT implement `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryCommitReceipt {
    pub operation: EntryMutationKind,
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub version_before: Option<i64>,
    pub version_after: Option<i64>,
    pub publication_before: Option<String>,
    pub publication_after: Option<String>,
    pub trashed_at: Option<String>,
    pub published_at: Option<String>,
}

/// Secondary post-commit projection work outcome, kept separate from durable DB commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProjectionOutcome {
    NotNeeded,
    Current,
    Failed { reason: String },
}

/// Bounded post-commit secondary work summary returned alongside committed responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecondaryOutcome {
    pub projection: ProjectionOutcome,
    pub hook_enqueued: u32,
    pub hook_dropped: u32,
}

impl SecondaryOutcome {
    pub fn none() -> Self {
        Self {
            projection: ProjectionOutcome::NotNeeded,
            hook_enqueued: 0,
            hook_dropped: 0,
        }
    }
}

/// Bounded metadata-only Trash entry view returned to authorized plugins (`content_trash_get`/`list`).
/// Never includes body, field values, SEO private data, or public URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrashEntryMetadata {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub title: String,
    pub slug: String,
    pub publication_status: String,
    pub version: i64,
    pub trashed_at: String,
    pub published_at: Option<String>,
}

/// Private storage-scope snapshot of a retained entry row loaded under a reserved writer
/// after authority and CAS validation.
#[derive(Debug, Clone)]
pub(in crate::storage) struct RetainedEntryIdentity {
    pub id: i64,
    pub type_key: String,
    pub locale: String,
    pub translation_group_id: String,
    pub slug: String,
    pub title: String,
    pub values_json: String,
    pub version: i64,
    pub publication_status: String,
    pub has_been_published: i64,
    pub trashed_at: Option<String>,
    pub published_at: Option<String>,
    pub primary_category_term_id: Option<i64>,
}

/// Retained row header for preflight checks that must distinguish 404 (absent),
/// 412 (stale version), and 409 (trashed target) without exposing entry body/values.
#[derive(Debug, Clone)]
pub(crate) struct RetainedEntryPreflight {
    pub id: i64,
    pub locale: String,
    pub translation_group_id: String,
    pub version: i64,
    pub trashed_at: Option<String>,
}

pub(crate) async fn load_retained_preflight_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryPreflight>> {
    let row: Option<(i64, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT id, locale, translation_group_id, version, trashed_at
           FROM content_entries
          WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(id, locale, translation_group_id, version, trashed_at)| RetainedEntryPreflight {
            id,
            locale,
            translation_group_id,
            version,
            trashed_at,
        },
    ))
}

pub(crate) async fn load_retained_preflight(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryPreflight>> {
    let mut conn = db.acquire().await?;
    load_retained_preflight_in_conn(&mut conn, type_key, entry_id).await
}

/// Validates that an authorized ordinary mutation targets an existing, active row
/// matching `expected_version`. Enforces CAS (412) before lifecycle state conflict (409).
pub(crate) async fn ensure_active_entry_version_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> ApiResult<RetainedEntryPreflight> {
    let Some(row) = load_retained_preflight_in_conn(conn, type_key, entry_id).await? else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if row.trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa cho đến khi khôi phục.",
        ));
    }
    Ok(row)
}

pub(crate) async fn ensure_active_entry_version(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
) -> ApiResult<RetainedEntryPreflight> {
    let mut conn = db.acquire().await?;
    ensure_active_entry_version_in_conn(&mut conn, type_key, entry_id, expected_version).await
}

pub(in crate::storage) async fn load_retained_entry_identity_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    type_key: &str,
    entry_id: i64,
) -> ApiResult<Option<RetainedEntryIdentity>> {
    type RowTuple = (
        i64,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<i64>,
    );
    let row: Option<RowTuple> = sqlx::query_as(
        "SELECT id, content_type_key, locale, translation_group_id, slug, title, values_json,
                version, publication_status, has_been_published, trashed_at, published_at,
                primary_category_term_id
           FROM content_entries
          WHERE content_type_key = ? AND id = ?",
    )
    .bind(type_key)
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(
        |(
            id,
            type_key,
            locale,
            translation_group_id,
            slug,
            title,
            values_json,
            version,
            publication_status,
            has_been_published,
            trashed_at,
            published_at,
            primary_category_term_id,
        )| RetainedEntryIdentity {
            id,
            type_key,
            locale,
            translation_group_id,
            slug,
            title,
            values_json,
            version,
            publication_status,
            has_been_published,
            trashed_at,
            published_at,
            primary_category_term_id,
        },
    ))
}

/// Returns `(kind, locale, active_and_declared)` for a single term ID (used by S03 hook scope checks).
pub(crate) async fn lookup_term_scope_in_conn(
    conn: &mut SqliteConnection,
    term_id: i64,
) -> ApiResult<Option<(String, String, bool)>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT t.kind, t.locale, d.provider_status, d.declaration_state
           FROM taxonomy_terms t
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE t.id = ?",
    )
    .bind(term_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(
        row.map(|(kind, locale, provider_status, declaration_state)| {
            (
                kind,
                locale,
                provider_status == "active" && declaration_state == "declared",
            )
        }),
    )
}

pub(in crate::storage) fn deserialize_non_null_option<'de, T, D>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyDeltaOp {
    Add,
    Remove,
    Replace,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataTaxonomyPatch {
    pub key: String,
    pub operation: TaxonomyDeltaOp,
    pub term_ids: Vec<i64>,
}

impl MetadataPatch {
    pub fn has_any_field(&self) -> bool {
        self.title.is_some()
            || self.slug.is_some()
            || self.publication_status.is_some()
            || self.published_at.is_some()
            || self.taxonomy.is_some()
    }
}

#[derive(Debug, Serialize)]
pub struct MetadataPatchResponse {
    #[serde(flatten)]
    pub row: crate::storage::entry_table::EntryTableRow,
    pub schema_token: String,
    pub secondary: SecondaryOutcome,
}

pub struct MetadataPreflightPlan {
    pub preflight_schema_token: String,
    pub effective: opendash_core::content::EffectiveContentSchema,
    pub entry_locale: String,
    pub translation_group_id: String,
    pub candidate_title: String,
    pub candidate_slug: String,
    pub candidate_values: BTreeMap<String, Value>,
    pub candidate_term_ids: Vec<i64>,
    pub candidate_status: String,
    pub explicit_published_at_utc: Option<String>,
    pub requested_title: bool,
    pub requested_slug: bool,
    pub requested_taxonomy_key: Option<String>,
    pub hook_modified_terms: bool,
}

pub(in crate::storage) fn validate_metadata_title(raw: &str) -> ApiResult<String> {
    if raw.chars().any(char::is_control) {
        return Err(ApiError::unprocessable(
            "Tiêu đề không được chứa ký tự điều khiển.",
        ));
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 {
        return Err(ApiError::unprocessable(
            "Tiêu đề phải có từ 1 đến 200 ký tự.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub(in crate::storage) fn validate_metadata_slug(raw: &str) -> ApiResult<String> {
    if !valid_kebab_segment_with_limit(raw, 64) {
        return Err(ApiError::unprocessable(
            "Slug không hợp lệ (chỉ gồm chữ thường, số, gạch ngang và tối đa 64 ký tự).",
        ));
    }
    Ok(raw.to_owned())
}

pub(in crate::storage) async fn resolve_taxonomy_delta_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    entry_locale: &str,
    delta: &MetadataTaxonomyPatch,
) -> ApiResult<(Vec<i64>, std::collections::BTreeSet<i64>)> {
    if !valid_content_type_key(&delta.key) {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Khóa phân loại không hợp lệ.",
        ));
    }
    let tax_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM taxonomy_definitions WHERE key = ?",
    )
    .bind(&delta.key)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((provider_status, declaration_state)) = tax_row else {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại không tồn tại hoặc không khả dụng.",
        ));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại hiện không hoạt động hoặc đã bị thu hồi.",
        ));
    }
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    if !eligible.iter().any(|k| k == &delta.key) {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Phân loại không được phép dùng cho loại nội dung này.",
        ));
    }
    if matches!(
        delta.operation,
        TaxonomyDeltaOp::Add | TaxonomyDeltaOp::Remove
    ) && delta.term_ids.is_empty()
    {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Thao tác thêm hoặc gỡ phân loại cần ít nhất một term_id.",
        ));
    }
    let max_for_kind = if delta.key == "category" { 32 } else { 64 };
    if delta.term_ids.len() > max_for_kind {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term vượt quá giới hạn của phân loại.",
        ));
    }
    let mut input_ids = std::collections::BTreeSet::new();
    for &tid in &delta.term_ids {
        if tid <= 0 {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Danh sách term_ids chứa ID không hợp lệ.",
            ));
        }
        if !input_ids.insert(tid) {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Danh sách term_ids không được chứa ID trùng lặp.",
            ));
        }
        let found: Option<(String, String, String, String)> = sqlx::query_as(
            "SELECT t.kind, t.locale, d.provider_status, d.declaration_state
               FROM taxonomy_terms t
               JOIN taxonomy_definitions d ON d.key = t.kind
              WHERE t.id = ?",
        )
        .bind(tid)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((kind, term_locale, ps, ds)) = found else {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term ID không tồn tại.",
            ));
        };
        if ps != "active" || ds != "declared" {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term thuộc phân loại không hoạt động.",
            ));
        }
        if kind != delta.key {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term ID không thuộc phân loại được chỉ định.",
            ));
        }
        if term_locale != entry_locale {
            return Err(ApiError::unprocessable(
                "invalid_taxonomy_delta: Term thuộc ngôn ngữ khác với nội dung.",
            ));
        }
    }

    let existing_rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, d.provider_status, d.declaration_state
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut other_ids = std::collections::BTreeSet::new();
    let mut other_active_eligible_count = 0usize;
    let mut current_kind_ids = std::collections::BTreeSet::new();
    for (tid, kind, ps, ds) in existing_rows {
        if kind == delta.key {
            current_kind_ids.insert(tid);
        } else {
            other_ids.insert(tid);
            if ps == "active" && ds == "declared" && eligible.iter().any(|k| k == &kind) {
                other_active_eligible_count += 1;
            }
        }
    }

    let next_kind_ids: std::collections::BTreeSet<i64> = match delta.operation {
        TaxonomyDeltaOp::Add => current_kind_ids.union(&input_ids).copied().collect(),
        TaxonomyDeltaOp::Remove => current_kind_ids.difference(&input_ids).copied().collect(),
        TaxonomyDeltaOp::Replace => input_ids,
    };

    if next_kind_ids.len() > max_for_kind {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term sau khi cập nhật vượt quá giới hạn của phân loại.",
        ));
    }
    if other_active_eligible_count + next_kind_ids.len() > 96 {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Tổng số term hoạt động của nội dung vượt quá giới hạn 96.",
        ));
    }

    let merged: Vec<i64> = other_ids.union(&next_kind_ids).copied().collect();
    Ok((merged, next_kind_ids))
}

pub(in crate::storage) async fn validate_hook_adjusted_terms_in_conn(
    conn: &mut SqliteConnection,
    type_key: &str,
    entry_id: i64,
    entry_locale: &str,
    allowed_key: &str,
    candidate_ids: &[i64],
) -> ApiResult<Vec<i64>> {
    let eligible = crate::storage::content::load_effective_taxonomies_in_tx(conn, type_key).await?;
    let existing_rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.kind, d.provider_status, d.declaration_state
           FROM entry_terms et
           JOIN taxonomy_terms t ON t.id = et.term_id
           JOIN taxonomy_definitions d ON d.key = t.kind
          WHERE et.entry_id = ?
          ORDER BY t.id ASC",
    )
    .bind(entry_id)
    .fetch_all(&mut *conn)
    .await?;

    let mut other_ids = std::collections::BTreeSet::new();
    let mut other_active_eligible_count = 0usize;
    for (tid, kind, ps, ds) in existing_rows {
        if kind != allowed_key {
            other_ids.insert(tid);
            if ps == "active" && ds == "declared" && eligible.iter().any(|k| k == &kind) {
                other_active_eligible_count += 1;
            }
        }
    }

    let mut next_kind_ids = std::collections::BTreeSet::new();
    for &tid in candidate_ids {
        if other_ids.contains(&tid) {
            continue;
        }
        let info = lookup_term_scope_in_conn(conn, tid).await?;
        let Some((kind, loc, active)) = info else {
            return Err(ApiError::unprocessable(
                "metadata_hook_scope_conflict: Plugin filter trả về term không tồn tại.",
            ));
        };
        if !active || kind != allowed_key || loc != entry_locale {
            return Err(ApiError::unprocessable(
                "metadata_hook_scope_conflict: Plugin filter cố gắng thay đổi phân loại ngoài phạm vi Sửa nhanh; hãy mở trình chỉnh sửa đầy đủ (Single Edit).",
            ));
        }
        next_kind_ids.insert(tid);
    }

    let max_for_kind = if allowed_key == "category" { 32 } else { 64 };
    if next_kind_ids.len() > max_for_kind || other_active_eligible_count + next_kind_ids.len() > 96
    {
        return Err(ApiError::unprocessable(
            "invalid_taxonomy_delta: Số lượng term sau khi filter vượt quá giới hạn cho phép.",
        ));
    }
    Ok(other_ids.union(&next_kind_ids).copied().collect())
}

pub(in crate::storage) async fn prepare_metadata_patch_inner(
    db: &SqlitePool,
    _actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
) -> ApiResult<MetadataPreflightPlan> {
    if !valid_content_type_key(type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if matches!(type_key, "pattern" | "section") {
        return Err(ApiError::forbidden(
            "entry_lifecycle_forbidden: Không hỗ trợ thao tác vòng đời chung cho Pattern hoặc Section.",
        ));
    }
    if entry_id < 1 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    let client_token = patch
        .expected_schema_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError(
                axum::http::StatusCode::PRECONDITION_REQUIRED,
                "missing_schema_token: Thiếu expected_schema_token.".into(),
            )
        })?;
    if !patch.has_any_field() {
        return Err(ApiError::unprocessable(
            "metadata_empty_patch: Cần chọn ít nhất một trường siêu dữ liệu để cập nhật.",
        ));
    }

    let mut tx = db.begin().await?;
    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_row else {
        return Err(ApiError::not_found("Không tìm thấy loại nội dung."));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Err(ApiError::conflict(
            "provider_inactive: Loại nội dung hiện không hoạt động hoặc đã bị thu hồi.",
        ));
    }
    let Some(effective) =
        crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, type_key).await?
    else {
        return Err(ApiError::conflict(
            "provider_inactive: Không thể tải schema hiện hành của loại nội dung.",
        ));
    };
    if client_token != effective.schema_token {
        return Err(ApiError::precondition_failed(
            "schema_version_conflict: Cấu trúc trường của loại nội dung đã thay đổi; hãy tải lại trước khi lưu.",
        ));
    }

    // Enforce CAS (412) before lifecycle conflict (409 entry_trashed).
    let Some(row) = load_retained_entry_identity_in_tx(&mut tx, type_key, entry_id).await? else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
        ));
    }
    if row.trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa cho đến khi khôi phục.",
        ));
    }

    // Verify locale manageability (matching Single Edit contract).
    let locale_row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&row.locale)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((locale_is_default, locale_deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung không còn tồn tại trong cấu hình.",
        ));
    };
    if locale_deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy dùng công cụ dọn ngôn ngữ mồ côi.",
        ));
    }
    if locale_is_default == 0 {
        let multilingual_enabled: i64 =
            sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
                .fetch_one(&mut *tx)
                .await?;
        if multilingual_enabled == 0 {
            return Err(ApiError::conflict(
                "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
            ));
        }
    }

    let candidate_title = match patch.title.as_deref() {
        Some(raw) => validate_metadata_title(raw)?,
        None => row.title.clone(),
    };
    let candidate_slug = match patch.slug.as_deref() {
        Some(raw) => validate_metadata_slug(raw)?,
        None => row.slug.clone(),
    };

    let is_draft_publish =
        effective.base.descriptor.settings.publication == ContentPublication::DraftPublish;
    let candidate_status = match patch.publication_status.as_deref() {
        Some(st) => {
            if !is_draft_publish {
                return Err(ApiError::conflict(
                    "publication_policy_unsupported: Loại nội dung lưu ngay không hỗ trợ đổi trạng thái xuất bản qua thao tác này.",
                ));
            }
            if st != "draft" && st != "published" {
                return Err(ApiError::unprocessable(
                    "publication_status chỉ hỗ trợ 'draft' hoặc 'published'.",
                ));
            }
            st.to_owned()
        }
        None => row.publication_status.clone(),
    };

    let explicit_published_at_utc = match &patch.published_at {
        Some(date_patch) => {
            if candidate_status != "published" && row.published_at.is_none() {
                return Err(ApiError::unprocessable(
                    "draft_cannot_set_published_at: Bản nháp chưa từng xuất bản không thể đặt ngày xuất bản nếu không xuất bản tường minh.",
                ));
            }
            let general = crate::storage::settings::get_general_in_conn(&mut tx).await?;
            Some(parse_and_convert_published_at(
                date_patch,
                &general.timezone,
            )?)
        }
        None => None,
    };

    let candidate_term_ids = match &patch.taxonomy {
        Some(delta) => {
            let (merged, _) =
                resolve_taxonomy_delta_in_conn(&mut tx, type_key, entry_id, &row.locale, delta)
                    .await?;
            merged
        }
        None => {
            sqlx::query_scalar(
                "SELECT term_id FROM entry_terms WHERE entry_id = ? ORDER BY term_id ASC",
            )
            .bind(entry_id)
            .fetch_all(&mut *tx)
            .await?
        }
    };

    let candidate_values: BTreeMap<String, Value> = serde_json::from_str(&row.values_json)
        .map_err(|_| ApiError::internal("Dữ liệu custom fields không hợp lệ."))?;
    tx.commit().await?;

    Ok(MetadataPreflightPlan {
        preflight_schema_token: effective.schema_token.clone(),
        effective,
        entry_locale: row.locale,
        translation_group_id: row.translation_group_id,
        candidate_title,
        candidate_slug,
        candidate_values,
        candidate_term_ids,
        candidate_status,
        explicit_published_at_utc,
        requested_title: patch.title.is_some(),
        requested_slug: patch.slug.is_some(),
        requested_taxonomy_key: patch.taxonomy.as_ref().map(|t| t.key.clone()),
        hook_modified_terms: false,
    })
}

/// Applies validated, scope-checked `filter:content/prepare_save` modifications to a metadata plan
/// and re-validates title/slug bounds before `before_save` and commit.
pub fn apply_prepare_save_to_metadata_plan(
    plan: &mut MetadataPreflightPlan,
    hook_patch: crate::hooks::ContentPrepareSavePatch,
) -> ApiResult<()> {
    if let Some(title) = hook_patch.title {
        plan.candidate_title = validate_metadata_title(&title)?;
    }
    if let Some(slug) = hook_patch.slug {
        plan.candidate_slug = validate_metadata_slug(&slug)?;
    }
    if let Some(term_ids) = hook_patch.term_ids {
        plan.candidate_term_ids = term_ids;
        plan.hook_modified_terms = true;
    }
    Ok(())
}
