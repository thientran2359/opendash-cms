use super::*;

/// Read-only snapshot preflight for plugin `content_restore` / `content_purge` confirmation creation.
/// Validates authority, plugin fence, active CPT, CAS (412 before 409), trashed state, locale, and
/// determines whether explicit host confirmation is required without holding a SQLite writer.
pub async fn preflight_plugin_lifecycle_confirmation(
    db: &SqlitePool,
    session: &SessionContext,
    command: &LifecycleCommand,
    fence: &PluginActionFence,
) -> ApiResult<LifecycleConfirmationPreview> {
    if !valid_content_type_key(&command.type_key) {
        return Err(ApiError::bad_request("Khóa loại nội dung không hợp lệ."));
    }
    if matches!(command.type_key.as_str(), "pattern" | "section") {
        return Err(ApiError::forbidden(
            "entry_lifecycle_forbidden: Không hỗ trợ thao tác vòng đời chung cho Pattern hoặc Section.",
        ));
    }
    if command.entry_id < 1 {
        return Err(ApiError::bad_request("ID nội dung không hợp lệ."));
    }
    if command.expected_version < 1 {
        return Err(ApiError::bad_request("expected_version không hợp lệ."));
    }
    if command.operation == LifecycleOperation::Purge && session.user.role != Role::Admin {
        return Err(ApiError::forbidden(
            "entry_purge_forbidden: Chỉ Quản trị viên được phép xóa vĩnh viễn nội dung.",
        ));
    }

    let mut tx = db.begin().await?;
    verify_live_session_and_role_in_tx(&mut tx, session).await?;
    verify_plugin_fence_in_tx(&mut tx, session, fence).await?;

    let ct_row: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(&command.type_key)
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

    let Some(row) =
        load_retained_entry_identity_in_tx(&mut tx, &command.type_key, command.entry_id).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy nội dung."));
    };
    if row.version != command.expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi thao tác.",
        ));
    }
    if row.trashed_at.is_none() {
        return Err(ApiError::conflict(match command.operation {
            LifecycleOperation::Purge => {
                "entry_not_trashed: Chỉ có thể xóa vĩnh viễn nội dung đã nằm trong Thùng rác."
            }
            _ => "entry_not_trashed: Nội dung hiện không nằm trong Thùng rác.",
        }));
    }

    let locale_row: Option<(i64, i64, Option<String>)> =
        sqlx::query_as("SELECT enabled, is_default, deleted_at FROM site_locales WHERE code = ?")
            .bind(&row.locale)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((locale_enabled, locale_is_default, locale_deleted_at)) = locale_row else {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy xử lý trong mục bảo trì ngôn ngữ.",
        ));
    };
    if locale_deleted_at.is_some() {
        return Err(ApiError::conflict(
            "locale_deleted: Ngôn ngữ của nội dung đã bị xóa; hãy xử lý trong mục bảo trì ngôn ngữ.",
        ));
    }
    let multilingual_enabled: i64 =
        sqlx::query_scalar("SELECT multilingual_enabled FROM site_locales_state WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
    if locale_is_default == 0 && multilingual_enabled == 0 {
        return Err(ApiError::conflict(
            "locale_parked: Ngôn ngữ phụ đang bị tạm dừng; hãy bật lại chế độ đa ngôn ngữ trước khi thao tác.",
        ));
    }

    let needs_confirmation = match command.operation {
        LifecycleOperation::Purge => true,
        LifecycleOperation::Restore => {
            if locale_enabled == 0 {
                return Err(ApiError::conflict(
                    "locale_disabled: Không thể khôi phục khi ngôn ngữ của nội dung đang tắt.",
                ));
            }
            let Some(effective) = crate::storage::content::load_effective_content_schema_in_tx(
                &mut *tx,
                &row.type_key,
            )
            .await?
            else {
                return Err(ApiError::conflict(
                    "provider_inactive: Không thể tải schema hiện hành của loại nội dung.",
                ));
            };
            validate_restore_compatibility_in_tx(&mut tx, &row, &effective).await?;
            effective.base.descriptor.settings.publication == ContentPublication::Immediate
                && effective.base.descriptor.settings.public
        }
        LifecycleOperation::Trash => false,
    };

    tx.commit().await?;
    Ok(LifecycleConfirmationPreview {
        type_key: row.type_key,
        entry_id: row.id,
        locale: row.locale,
        title: row.title,
        slug: row.slug,
        version: row.version,
        needs_confirmation,
    })
}

/// Authorized plugin metadata-only single Trash lookup (`content_trash_get`).
/// Scoped to active ordinary CPTs and currently manageable (non-deleted) locales only.
pub async fn get_trashed_entry_metadata_for_host(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    locale: Option<&str>,
) -> ApiResult<Option<TrashEntryMetadata>> {
    if !valid_content_type_key(type_key) || matches!(type_key, "pattern" | "section") {
        return Ok(None);
    }
    if entry_id < 1 {
        return Ok(None);
    }
    let mut tx = db.begin().await?;
    let ct_active: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_active else {
        return Ok(None);
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Ok(None);
    }
    let resolved_locale = match locale {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };
    type Row = (
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
    );
    let sql = format!(
        "SELECT e.id, e.content_type_key, e.locale, e.title, e.slug,
                e.publication_status, e.version, e.trashed_at, e.published_at
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND e.id = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}"
    );
    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(type_key)
        .bind(entry_id)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(row.map(
        |(
            id,
            type_key,
            locale,
            title,
            slug,
            publication_status,
            version,
            trashed_at,
            published_at,
        )| {
            TrashEntryMetadata {
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            }
        },
    ))
}

/// Authorized plugin metadata-only Trash list (`content_trash_list`) in one read snapshot.
/// Deterministic ordering: `trashed_at DESC, id DESC`.
pub async fn list_trashed_entries_metadata_for_host(
    db: &SqlitePool,
    type_key: &str,
    locale: Option<&str>,
    limit: i64,
    offset: i64,
) -> ApiResult<(Vec<TrashEntryMetadata>, i64)> {
    if !valid_content_type_key(type_key) || matches!(type_key, "pattern" | "section") {
        return Ok((Vec::new(), 0));
    }
    let clamped_limit = limit.clamp(1, 50);
    let clamped_offset = offset.clamp(0, 10_000);

    let mut tx = db.begin().await?;
    let ct_active: Option<(String, String)> = sqlx::query_as(
        "SELECT provider_status, declaration_state FROM content_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((provider_status, declaration_state)) = ct_active else {
        return Ok((Vec::new(), 0));
    };
    if provider_status != "active" || declaration_state != "declared" {
        return Ok((Vec::new(), 0));
    }
    let resolved_locale = match locale {
        Some(raw) => Some(
            crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, Some(raw)).await?,
        ),
        None => None,
    };

    let count_sql = format!(
        "SELECT COUNT(*)
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}"
    );
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .fetch_one(&mut *tx)
        .await?;

    type Row = (
        i64,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
    );
    let rows_sql = format!(
        "SELECT e.id, e.content_type_key, e.locale, e.title, e.slug,
                e.publication_status, e.version, e.trashed_at, e.published_at
           FROM content_entries e
          WHERE e.content_type_key = ?
            AND {TRASHED_ENTRY_PREDICATE}
            AND (? IS NULL OR e.locale = ?)
            AND {MANAGEABLE_LOCALE_EXISTS_SQL}
          ORDER BY e.trashed_at DESC, e.id DESC
          LIMIT ? OFFSET ?"
    );
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(rows_sql))
        .bind(type_key)
        .bind(resolved_locale.as_deref())
        .bind(resolved_locale.as_deref())
        .bind(clamped_limit)
        .bind(clamped_offset)
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;

    let items = rows
        .into_iter()
        .map(
            |(
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            )| TrashEntryMetadata {
                id,
                type_key,
                locale,
                title,
                slug,
                publication_status,
                version,
                trashed_at,
                published_at,
            },
        )
        .collect();
    Ok((items, total))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedAtPatch {
    pub local_datetime: String,
    pub timezone: String,
}

/// Sparse narrow metadata patch shared by Quick Edit and Bulk Taxonomy (`PATCH .../metadata`).
/// Omission means retain; explicit `null` and unknown fields are rejected at deserialization.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPatch {
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub expected_schema_token: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub publication_status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub published_at: Option<PublishedAtPatch>,
    #[serde(default, deserialize_with = "deserialize_non_null_option")]
    pub taxonomy: Option<MetadataTaxonomyPatch>,
}

/// Strictly parses ASCII `YYYY-MM-DDTHH:mm:ss` (mandatory seconds, no suffix/fraction),
/// enforces echoed site IANA timezone, rejects nonexistent/ambiguous DST wall times
/// via `jiff::tz::Disambiguation::Reject`, rejects future timestamps, and returns
/// canonical SQLite UTC `"YYYY-MM-DD HH:MM:SS"`.
pub(crate) fn parse_and_convert_published_at(
    patch: &PublishedAtPatch,
    current_site_timezone: &str,
) -> ApiResult<String> {
    if patch.timezone.trim() != current_site_timezone {
        return Err(ApiError::conflict(
            "timezone_changed: Múi giờ hệ thống đã thay đổi; hãy tải lại biểu mẫu trước khi lưu ngày xuất bản.",
        ));
    }
    let raw = patch.local_datetime.as_str();
    let b = raw.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || !b[0..4].iter().all(u8::is_ascii_digit)
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || !b[8..10].iter().all(u8::is_ascii_digit)
        || !b[11..13].iter().all(u8::is_ascii_digit)
        || !b[14..16].iter().all(u8::is_ascii_digit)
        || !b[17..19].iter().all(u8::is_ascii_digit)
    {
        return Err(ApiError::unprocessable(
            "invalid_published_at: Ngày xuất bản phải đúng định dạng YYYY-MM-DDTHH:mm:ss (bắt buộc có giây, không có hậu tố múi giờ).",
        ));
    }
    let year: i16 = raw[0..4].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let month: i8 = raw[5..7].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let day: i8 = raw[8..10].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let hour: i8 = raw[11..13].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let minute: i8 = raw[14..16].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let second: i8 = raw[17..19].parse().map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    if year < 1970 {
        return Err(ApiError::unprocessable(
            "invalid_published_at: Năm của ngày xuất bản phải từ 1970 trở đi.",
        ));
    }
    let date = jiff::civil::Date::new(year, month, day).map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let time = jiff::civil::Time::new(hour, minute, second, 0).map_err(|_| {
        ApiError::unprocessable("invalid_published_at: Ngày xuất bản không hợp lệ theo lịch dương.")
    })?;
    let civil_dt = jiff::civil::DateTime::from_parts(date, time);
    let tz = jiff::tz::TimeZone::get(current_site_timezone).map_err(|_| {
        ApiError::internal("Múi giờ hệ thống không tồn tại trong cơ sở dữ liệu IANA.")
    })?;
    let utc_ts = tz
        .to_ambiguous_timestamp(civil_dt)
        .disambiguate(jiff::tz::Disambiguation::Reject)
        .map_err(|_| {
            ApiError::unprocessable(
                "invalid_published_at: Thời điểm cục bộ không tồn tại hoặc mơ hồ do chuyển giờ mùa hè (DST); hãy chọn thời điểm hợp lệ không mơ hồ.",
            )
        })?;
    if utc_ts > jiff::Timestamp::now() {
        return Err(ApiError::unprocessable(
            "published_at_in_future: Không hỗ trợ đặt ngày xuất bản trong tương lai.",
        ));
    }
    let utc_dt = utc_ts.to_zoned_datetime(jiff::tz::TimeZone::UTC).datetime();
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        utc_dt.year(),
        utc_dt.month(),
        utc_dt.day(),
        utc_dt.hour(),
        utc_dt.minute(),
        utc_dt.second(),
    ))
}

/// Read-only preflight for `PATCH .../entries/{id}/metadata`.
/// Holds no SQLite writer across plugin `prepare_save` / `before_save` hook execution.
pub async fn prepare_metadata_patch(
    db: &SqlitePool,
    actor: &SessionContext,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    patch: &MetadataPatch,
) -> ApiResult<MetadataPreflightPlan> {
    let started = std::time::Instant::now();
    let res =
        prepare_metadata_patch_inner(db, actor, type_key, entry_id, expected_version, patch).await;
    let storage_us = started.elapsed().as_micros();
    let role = actor.user.role.as_str();
    match &res {
        Ok(_) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_prepare type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=ok status=200 storage_us={storage_us}",
                actor.user.id,
            );
        }
        Err(err) => {
            eprintln!(
                "entry_lifecycle_diag op=metadata_prepare type_key={type_key} entry_id={entry_id} actor_id={} actor_role={role} expected_version={expected_version} version_after=0 outcome=err status={} storage_us={storage_us}",
                actor.user.id,
                err.0.as_u16(),
            );
        }
    }
    res
}
