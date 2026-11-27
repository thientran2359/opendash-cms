use super::*;

pub(in crate::storage) fn validate_sample_manifest(
    sample: &ThemeSampleManifest,
    file_digests: &BTreeMap<String, String>,
    schema: &[FieldSpec],
) -> ApiResult<()> {
    if !valid_object_key(&sample.sample_key) || sample.sample_key.len() > 64 {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "sample_key không hợp lệ.".into(),
        ));
    }
    if sample.media.len() > MAX_SAMPLE_MEDIA
        || sample.pages.len() > MAX_SAMPLE_PAGES
        || sample.menus.len() > MAX_SAMPLE_MENUS
        || sample.settings.len() > MAX_SAMPLE_SETTINGS
    {
        return Err(ApiError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Số lượng phần tử dữ liệu mẫu vượt quá giới hạn cho phép.".into(),
        ));
    }

    let mut media_keys = HashSet::new();
    for m in &sample.media {
        if !valid_object_key(&m.object_key) || !media_keys.insert(m.object_key.as_str()) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "object_key của sample media không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        let safe_rel = validate_theme_relative_path(&m.path).map_err(|_| {
            ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Đường dẫn media trong sample không hợp lệ.".into(),
            )
        })?;
        if !file_digests.contains_key(&safe_rel) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "File media được khai báo trong sample không tồn tại trong revision theme.".into(),
            ));
        }
    }

    let mut page_keys = HashSet::new();
    for p in &sample.pages {
        if !valid_object_key(&p.object_key) || !page_keys.insert(p.object_key.as_str()) {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "object_key của sample page không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        if !valid_kebab_segment_with_limit(&p.slug, 64)
            || !valid_snake_key(&p.content_type)
            || p.title.trim().is_empty()
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Thông tin slug/title/content_type của sample page không hợp lệ.".into(),
            ));
        }
        for mref in &p.media_refs {
            if !media_keys.contains(mref.as_str()) {
                return Err(ApiError(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Sample page tham chiếu tới media_ref không tồn tại trong sample.".into(),
                ));
            }
        }
    }

    let mut menu_keys = HashSet::new();
    for menu in &sample.menus {
        if !valid_object_key(&menu.object_key)
            || !menu_keys.insert(menu.object_key.as_str())
            || !valid_snake_key(&menu.menu_key)
            || menu.title.trim().is_empty()
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Khai báo sample menu không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        for entry in &menu.items {
            match entry.target_kind.as_str() {
                "entry" => {
                    let Some(target) = entry.target_object_key.as_deref() else {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Menu entry kiểu 'entry' thiếu target_object_key.".into(),
                        ));
                    };
                    if !page_keys.contains(target) {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Sample menu tham chiếu tới page không tồn tại trong sample.".into(),
                        ));
                    }
                }
                "external" => {
                    if entry.url.as_deref().unwrap_or("").trim().is_empty() {
                        return Err(ApiError(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Menu entry kiểu 'external' thiếu url.".into(),
                        ));
                    }
                }
                _ => {
                    return Err(ApiError(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Loại target menu sample không được hỗ trợ.".into(),
                    ));
                }
            }
        }
    }

    let mut setting_keys = HashSet::new();
    for s in &sample.settings {
        if !valid_object_key(&s.object_key)
            || !setting_keys.insert(s.object_key.as_str())
            || !valid_snake_key(&s.option_key)
        {
            return Err(ApiError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Khai báo sample setting không hợp lệ hoặc trùng lặp.".into(),
            ));
        }
        if let Some(mref) = s.media_ref.as_deref() {
            if !media_keys.contains(mref) {
                return Err(ApiError(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Sample setting tham chiếu tới media_ref không tồn tại trong sample.".into(),
                ));
            }
        }
    }

    validate_sample_option_values(sample, schema).map_err(ApiError::unprocessable)?;
    Ok(())
}

pub(in crate::storage) async fn has_tombstone(
    db: &SqlitePool,
    theme_id: &str,
    sample_key: &str,
    kind: SampleGroup,
    object_key: &str,
) -> ApiResult<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM import_tombstones
            WHERE theme_id = ? AND sample_key = ? AND resource_kind = ? AND object_key = ?
        )",
    )
    .bind(theme_id)
    .bind(sample_key)
    .bind(kind.as_str())
    .bind(object_key)
    .fetch_one(db)
    .await
    .map_err(Into::into)
}

pub(in crate::storage) async fn has_provenance(
    db: &SqlitePool,
    theme_id: &str,
    sample_key: &str,
    kind: SampleGroup,
    object_key: &str,
) -> ApiResult<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM import_provenance
            WHERE theme_id = ? AND sample_key = ? AND resource_kind = ? AND object_key = ?
        )",
    )
    .bind(theme_id)
    .bind(sample_key)
    .bind(kind.as_str())
    .bind(object_key)
    .fetch_one(db)
    .await
    .map_err(Into::into)
}
