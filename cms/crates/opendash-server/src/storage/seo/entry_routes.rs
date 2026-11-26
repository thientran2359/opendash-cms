use super::*;

pub(crate) async fn verify_og_media_exists_in_tx(
    conn: &mut SqliteConnection,
    og_image_id: Option<&str>,
) -> ApiResult<()> {
    let Some(mid) = og_image_id else {
        return Ok(());
    };
    let exists: Option<String> =
        sqlx::query_scalar("SELECT id FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'")
            .bind(mid)
            .fetch_optional(&mut *conn)
            .await?;
    if exists.is_none() {
        return Err(ApiError::unprocessable(
            "Ảnh OG không tồn tại trong thư viện media.",
        ));
    }
    Ok(())
}

pub async fn put_entry_seo(
    db: &SqlitePool,
    type_key: &str,
    entry_id: i64,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<EntrySeoSnapshot> {
    let clean = validate_seo_input(&input)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let current_row: Option<(i64, Option<String>)> = sqlx::query_as(
            "SELECT version, trashed_at FROM content_entries WHERE content_type_key = ? AND id = ?",
        )
        .bind(type_key)
        .bind(entry_id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((current_ver, trashed_at)) = current_row else {
            return Err(ApiError::not_found("Không tìm thấy nội dung."));
        };
        if current_ver != expected_version {
            return Err(ApiError::precondition_failed(
                "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
            ));
        }
        if trashed_at.is_some() {
            return Err(ApiError::conflict(
                "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa SEO cho đến khi khôi phục.",
            ));
        }
        verify_og_media_exists_in_tx(&mut conn, clean.og_image_id.as_deref()).await?;

        let bumped = sqlx::query(
            "UPDATE content_entries
                SET version = version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
              WHERE id = ? AND version = ? AND trashed_at IS NULL",
        )
        .bind(entry_id)
        .bind(expected_version)
        .execute(&mut *conn)
        .await?;
        if bumped.rows_affected() == 0 {
            return Err(ApiError::precondition_failed(
                "Phiên bản nội dung đã thay đổi. Hãy tải lại trước khi lưu.",
            ));
        }

        sqlx::query(
            "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(entry_id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                og_image_id = excluded.og_image_id,
                canonical_url = excluded.canonical_url,
                noindex = excluded.noindex",
        )
        .bind(entry_id)
        .bind(&clean.title)
        .bind(&clean.description)
        .bind(&clean.og_image_id)
        .bind(&clean.canonical_url)
        .bind(i64::from(clean.noindex))
        .execute(&mut *conn)
        .await?;

        sync_entry_seo_media_ref_in_tx(&mut conn, entry_id).await?;

        Ok(EntrySeoSnapshot {
            entry_version: expected_version + 1,
            title: clean.title,
            description: clean.description,
            og_image_id: clean.og_image_id,
            canonical_url: clean.canonical_url,
            noindex: clean.noindex,
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

pub async fn put_home_seo_for_locale(
    db: &SqlitePool,
    requested_locale: Option<&str>,
    expected_version: i64,
    input: SeoMetadataInput,
) -> ApiResult<HomeSeoSnapshot> {
    let clean = validate_seo_input(&input)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let resolved_locale =
            crate::storage::settings::resolve_requested_locale_in_conn(&mut conn, requested_locale)
                .await?;
        verify_og_media_exists_in_tx(&mut conn, clean.og_image_id.as_deref()).await?;
        let current_ver: Option<i64> =
            sqlx::query_scalar("SELECT version FROM site_home_seo WHERE locale = ?")
                .bind(&resolved_locale)
                .fetch_optional(&mut *conn)
                .await?;
        let new_version = match current_ver {
            None => {
                if expected_version != 1 {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                sqlx::query(
                    "INSERT INTO site_home_seo
                        (locale, title, description, og_image_id, canonical_url, noindex, version)
                     VALUES (?, ?, ?, ?, ?, ?, 2)",
                )
                .bind(&resolved_locale)
                .bind(&clean.title)
                .bind(&clean.description)
                .bind(&clean.og_image_id)
                .bind(&clean.canonical_url)
                .bind(i64::from(clean.noindex))
                .execute(&mut *conn)
                .await?;
                2
            }
            Some(ver) => {
                if ver != expected_version {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                let updated = sqlx::query(
                    "UPDATE site_home_seo
                        SET title = ?, description = ?, og_image_id = ?, canonical_url = ?, noindex = ?,
                            version = version + 1
                      WHERE locale = ? AND version = ?",
                )
                .bind(&clean.title)
                .bind(&clean.description)
                .bind(&clean.og_image_id)
                .bind(&clean.canonical_url)
                .bind(i64::from(clean.noindex))
                .bind(&resolved_locale)
                .bind(expected_version)
                .execute(&mut *conn)
                .await?;
                if updated.rows_affected() == 0 {
                    return Err(ApiError::precondition_failed(
                        "Phiên bản SEO trang chủ đã thay đổi. Hãy tải lại trước khi lưu.",
                    ));
                }
                expected_version + 1
            }
        };

        Ok(HomeSeoSnapshot {
            version: new_version,
            title: clean.title,
            description: clean.description,
            og_image_id: clean.og_image_id,
            canonical_url: clean.canonical_url,
            noindex: clean.noindex,
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

pub(crate) async fn apply_entry_route_patch_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    patch: &PatchField<String>,
    record_old_redirect: bool,
) -> ApiResult<Option<String>> {
    let old_custom: Option<String> =
        sqlx::query_scalar("SELECT path FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;

    let normalized_new = match patch {
        PatchField::Omitted => return Ok(old_custom),
        PatchField::Clear => None,
        PatchField::Set(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(validate_route_claim_in_tx(conn, trimmed, Some(entry_id)).await?)
            }
        }
    };

    if normalized_new == old_custom {
        return Ok(old_custom);
    }

    if record_old_redirect || normalized_new.is_some() {
        let entry_locale: String =
            sqlx::query_scalar("SELECT locale FROM content_entries WHERE id = ?")
                .bind(entry_id)
                .fetch_one(&mut *conn)
                .await?;
        if record_old_redirect && let Some(old_path) = old_custom {
            sqlx::query(
                "INSERT OR IGNORE INTO entry_redirects (locale, source_path, entry_id) VALUES (?, ?, ?)",
            )
            .bind(&entry_locale)
            .bind(&old_path)
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
        }

        match &normalized_new {
            Some(new_path) => {
                sqlx::query(
                    "DELETE FROM entry_redirects WHERE locale = ? AND source_path = ? AND entry_id = ?",
                )
                .bind(&entry_locale)
                .bind(new_path)
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO entry_public_paths (entry_id, locale, path)
                     VALUES (?, ?, ?)
                     ON CONFLICT(entry_id) DO UPDATE SET
                         locale = excluded.locale,
                         path = excluded.path",
                )
                .bind(entry_id)
                .bind(&entry_locale)
                .bind(new_path)
                .execute(&mut *conn)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM entry_public_paths WHERE entry_id = ?")
                    .bind(entry_id)
                    .execute(&mut *conn)
                    .await?;
            }
        }
    } else {
        sqlx::query("DELETE FROM entry_public_paths WHERE entry_id = ?")
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
    }

    Ok(normalized_new)
}

pub(crate) async fn apply_entry_seo_patch_in_tx(
    conn: &mut SqliteConnection,
    entry_id: i64,
    entry_version: i64,
    patch: &PatchField<SeoMetadataInput>,
) -> ApiResult<Option<EntrySeoSnapshot>> {
    match patch {
        PatchField::Omitted => {
            let row: Option<(
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                i64,
            )> = sqlx::query_as(
                "SELECT title, description, og_image_id, canonical_url, noindex
                     FROM entry_seo WHERE entry_id = ?",
            )
            .bind(entry_id)
            .fetch_optional(&mut *conn)
            .await?;
            Ok(row.map(
                |(title, description, og_image_id, canonical_url, noindex)| EntrySeoSnapshot {
                    entry_version,
                    title,
                    description,
                    og_image_id,
                    canonical_url,
                    noindex: noindex != 0,
                },
            ))
        }
        PatchField::Clear => {
            sqlx::query(
                "DELETE FROM seo_public_media_refs WHERE source_kind = 'entry_seo' AND source_id = ?",
            )
            .bind(entry_id)
            .execute(&mut *conn)
            .await?;
            sqlx::query("DELETE FROM entry_seo WHERE entry_id = ?")
                .bind(entry_id)
                .execute(&mut *conn)
                .await?;
            Ok(None)
        }
        PatchField::Set(raw_seo) => {
            let clean = validate_seo_input(raw_seo)?;
            verify_og_media_exists_in_tx(conn, clean.og_image_id.as_deref()).await?;
            sqlx::query(
                "INSERT INTO entry_seo (entry_id, title, description, og_image_id, canonical_url, noindex)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(entry_id) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    og_image_id = excluded.og_image_id,
                    canonical_url = excluded.canonical_url,
                    noindex = excluded.noindex",
            )
            .bind(entry_id)
            .bind(&clean.title)
            .bind(&clean.description)
            .bind(&clean.og_image_id)
            .bind(&clean.canonical_url)
            .bind(i64::from(clean.noindex))
            .execute(&mut *conn)
            .await?;
            sync_entry_seo_media_ref_in_tx(conn, entry_id).await?;
            Ok(Some(EntrySeoSnapshot {
                entry_version,
                title: clean.title,
                description: clean.description,
                og_image_id: clean.og_image_id,
                canonical_url: clean.canonical_url,
                noindex: clean.noindex,
            }))
        }
    }
}
