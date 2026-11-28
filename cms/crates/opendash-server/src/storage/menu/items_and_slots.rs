use super::*;

pub async fn get_slot_version_for_locale_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    locale: &str,
) -> ApiResult<i64> {
    let ver: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM site_menu_slot_versions WHERE theme_id = ? AND locale = ?",
    )
    .bind(theme_id)
    .bind(locale)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(ver.unwrap_or(1))
}

pub async fn bump_slot_version_for_locale_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    locale: &str,
) -> ApiResult<i64> {
    let ver: i64 = sqlx::query_scalar(
        "INSERT INTO site_menu_slot_versions (theme_id, locale, version)
         VALUES (?, ?, 2)
         ON CONFLICT(theme_id, locale) DO UPDATE SET version = version + 1
         RETURNING version",
    )
    .bind(theme_id)
    .bind(locale)
    .fetch_one(&mut *conn)
    .await?;
    Ok(ver)
}

pub async fn list_slot_assignments_for_locale_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    locale: &str,
) -> ApiResult<BTreeMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT slot_key, menu_id
           FROM site_menu_slot_assignments
          WHERE theme_id = ? AND locale = ?
          ORDER BY slot_key ASC",
    )
    .bind(theme_id)
    .bind(locale)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().collect())
}

pub(crate) fn normalize_menu_path(raw: &str) -> String {
    let no_frag = raw.split('#').next().unwrap_or("");
    let no_query = no_frag.split('?').next().unwrap_or("");
    let trimmed = no_query.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub(in crate::storage) async fn resolve_slot_menus_for_locale_and_path_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    locale: &str,
    current_path: Option<&str>,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let assignments = list_slot_assignments_for_locale_in_tx(conn, theme_id, locale).await?;
    let mut out = BTreeMap::new();
    for (slot_key, menu_id) in assignments {
        if out.len() >= opendash_core::builder::MAX_MENU_LOADS_PER_REQUEST {
            break;
        }
        if let Some(view) =
            resolve_menu_view_for_locale_and_path_in_tx(conn, menu_id, current_path, Some(locale))
                .await?
        {
            out.insert(slot_key, view);
        }
    }
    Ok(out)
}

pub async fn delete_menu(db: &SqlitePool, menu_id: i64, expected_version: i64) -> ApiResult<()> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let menu = get_menu_record_in_tx(&mut conn, menu_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Không tìm thấy menu."))?;
        if menu.version != expected_version {
            return Err(ApiError::precondition_failed(
                "Menu đã được cập nhật ở phiên bản khác.",
            ));
        }
        let affected_slots: Vec<(String, String)> = sqlx::query_as(
            "SELECT DISTINCT theme_id, locale FROM site_menu_slot_assignments WHERE menu_id = ?",
        )
        .bind(menu_id)
        .fetch_all(&mut *conn)
        .await?;
        let deleted = sqlx::query("DELETE FROM site_menus WHERE id = ? AND version = ?")
            .bind(menu_id)
            .bind(expected_version)
            .execute(&mut *conn)
            .await?;
        if deleted.rows_affected() == 0 {
            return Err(ApiError::precondition_failed("Menu đã thay đổi đồng thời."));
        }
        for (theme_id, locale) in affected_slots {
            bump_slot_version_for_locale_in_tx(&mut conn, &theme_id, &locale).await?;
        }
        Ok::<_, ApiError>(())
    }
    .await;
    match res {
        Ok(()) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(())
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}

pub async fn get_menu_slots_for_locale(
    db: &SqlitePool,
    theme_id: &str,
    requested_locale: Option<&str>,
) -> ApiResult<Option<MenuSlotsSnapshot>> {
    let mut tx = db.begin().await?;
    let resolved_locale =
        crate::storage::settings::resolve_requested_locale_in_conn(&mut tx, requested_locale)
            .await?;
    let active: Option<(String, String)> = sqlx::query_as(
        "SELECT r.theme_id, r.manifest_json
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?;

    let (is_active, manifest_json) = if let Some((active_id, active_manifest)) = active
        && active_id == theme_id
    {
        (true, active_manifest)
    } else {
        let rev_manifest: Option<String> = sqlx::query_scalar(
            "SELECT manifest_json
               FROM theme_revisions
              WHERE theme_id = ?
              ORDER BY revision_id DESC
              LIMIT 1",
        )
        .bind(theme_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(manifest) = rev_manifest else {
            tx.commit().await?;
            return Ok(None);
        };
        (false, manifest)
    };

    let available_slots = extract_declared_menu_slots(&manifest_json);
    let version = get_slot_version_for_locale_in_tx(&mut tx, theme_id, &resolved_locale).await?;
    let assignments =
        list_slot_assignments_for_locale_in_tx(&mut tx, theme_id, &resolved_locale).await?;
    tx.commit().await?;
    Ok(Some(MenuSlotsSnapshot {
        theme_id: theme_id.to_owned(),
        version,
        is_active,
        available_slots,
        assignments,
        locale: requested_locale.map(|_| resolved_locale),
    }))
}

pub async fn put_menu_slots_for_locale(
    db: &SqlitePool,
    theme_id: &str,
    requested_locale: Option<&str>,
    expected_version: i64,
    assignments: &BTreeMap<String, Option<i64>>,
) -> ApiResult<MenuSlotsSnapshot> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let resolved_locale =
            crate::storage::settings::resolve_requested_locale_in_conn(&mut conn, requested_locale)
                .await?;
        let theme_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM theme_revisions WHERE theme_id = ? LIMIT 1")
                .bind(theme_id)
                .fetch_optional(&mut *conn)
                .await?;
        if theme_exists.is_none() {
            return Err(ApiError::not_found("Không tìm thấy theme."));
        }

        let active: Option<(String, String)> = sqlx::query_as(
            "SELECT r.theme_id, r.manifest_json
               FROM theme_active_selection a
               JOIN theme_revisions r ON r.revision_id = a.revision_id
              WHERE a.id = 1",
        )
        .fetch_optional(&mut *conn)
        .await?;
        let Some((active_id, active_manifest)) = active else {
            return Err(ApiError::conflict("Chưa có theme nào đang kích hoạt."));
        };
        if active_id != theme_id {
            return Err(ApiError::conflict(
                "Theme không phải revision đang kích hoạt.",
            ));
        }

        let available_slots = extract_declared_menu_slots(&active_manifest);
        for slot_key in assignments.keys() {
            validate_menu_key(slot_key)?;
            if !available_slots.contains(slot_key) {
                return Err(ApiError::conflict(
                    "Slot menu không tồn tại trong revision đang kích hoạt.",
                ));
            }
        }

        let current_ver =
            get_slot_version_for_locale_in_tx(&mut conn, theme_id, &resolved_locale).await?;
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Slot menu đã thay đổi ở phiên bản khác.",
            ));
        }

        for (slot_key, menu_id) in assignments {
            if let Some(mid) = menu_id {
                let exists: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM site_menus WHERE id = ?")
                        .bind(mid)
                        .fetch_optional(&mut *conn)
                        .await?;
                if exists.is_none() {
                    return Err(ApiError::not_found("Không tìm thấy menu để gắn slot."));
                }
                sqlx::query(
                    "INSERT INTO site_menu_slot_assignments (theme_id, locale, slot_key, menu_id)
                     VALUES (?, ?, ?, ?)
                     ON CONFLICT(theme_id, locale, slot_key) DO UPDATE SET menu_id = excluded.menu_id",
                )
                .bind(theme_id)
                .bind(&resolved_locale)
                .bind(slot_key)
                .bind(mid)
                .execute(&mut *conn)
                .await?;
            } else {
                sqlx::query(
                    "DELETE FROM site_menu_slot_assignments
                      WHERE theme_id = ? AND locale = ? AND slot_key = ?",
                )
                .bind(theme_id)
                .bind(&resolved_locale)
                .bind(slot_key)
                .execute(&mut *conn)
                .await?;
            }
        }

        let new_ver =
            bump_slot_version_for_locale_in_tx(&mut conn, theme_id, &resolved_locale).await?;
        let updated_assignments =
            list_slot_assignments_for_locale_in_tx(&mut conn, theme_id, &resolved_locale).await?;
        Ok::<_, ApiError>(MenuSlotsSnapshot {
            theme_id: theme_id.to_owned(),
            version: new_ver,
            is_active: true,
            available_slots,
            assignments: updated_assignments,
            locale: requested_locale.map(|_| resolved_locale),
        })
    }
    .await;
    match res {
        Ok(out) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            crate::cache::SiteCacheEpoch::global().bump();
            Ok(out)
        }
        Err(err) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(err)
        }
    }
}
