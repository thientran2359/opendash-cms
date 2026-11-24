//! Admin-only SITE-01/C3 API, with ETag/CAS and no-store responses.
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;
use std::{collections::BTreeMap, sync::atomic::Ordering};

use crate::{
    app::AppState,
    auth::{require_admin, require_admin_with_csrf, require_session},
    error::{ApiError, ApiResult},
    storage::permalinks::{self, PermalinkPutError, PermalinkSettings, PermalinkSettingsInput},
    storage::settings::{
        self, CreateLocaleInput, GeneralSettings, GeneralSettingsInput, GlobalSiteOptionsOutcome,
        GlobalSiteOptionsSnapshot, LocaleMutationError, LocaleQuery, PublicSaveOutcome,
        PublicSiteSettings, PublicSiteSettingsInput, ReadingSettings, ReadingSettingsInput,
        SaveOutcome, SiteLocalesSnapshot, ThemeOptionsOutcome, ThemeOptionsSnapshot,
        UpdateLocaleInput,
    },
};

pub(crate) fn no_store_headers(version: Option<i64>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(version) = version {
        let etag = HeaderValue::from_str(&format!("\"v{version}\""))
            .expect("integer ETag header is always valid");
        headers.insert(header::ETAG, etag);
    }
    headers
}

fn settings_response(settings: GeneralSettings) -> Response {
    (no_store_headers(Some(settings.version)), Json(settings)).into_response()
}

pub(crate) fn parse_if_match(headers: &HeaderMap) -> ApiResult<i64> {
    let value = headers
        .get(header::IF_MATCH)
        .ok_or_else(|| ApiError::precondition_required("Cần If-Match để lưu cài đặt."))?
        .to_str()
        .map_err(|_| ApiError::bad_request("If-Match không hợp lệ."))?;
    value
        .strip_prefix("\"v")
        .and_then(|value| value.strip_suffix('"'))
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|version| *version >= 1)
        .ok_or_else(|| ApiError::bad_request("If-Match phải có định dạng \"vN\"."))
}

fn map_storage_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref db_error) = error {
        if matches!(db_error.code().as_deref(), Some("5" | "6" | "517")) {
            return ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "SQLite đang bận, vui lòng thử lại.".into(),
            );
        }
    }
    error.into()
}

pub async fn general_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<LocaleQuery>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let current = settings::get_general_for_locale(&db, query.locale.as_deref()).await?;
    Ok(settings_response(current))
}

pub async fn timezones_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    Ok((
        no_store_headers(None),
        Json(json!({"version": settings::TZDB_VERSION, "items": settings::timezone_catalog()})),
    )
        .into_response())
}

/// Serialize saves through the post-commit cache store so concurrent requests
/// cannot publish an older CMS language after a newer transaction commits.
/// One process serves exactly one site.
static GENERAL_SAVE_CACHE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn admin_i18n_bootstrap(State(db): State<SqlitePool>) -> ApiResult<Response> {
    let bootstrap = settings::get_admin_i18n_bootstrap(&db).await?;
    Ok((no_store_headers(None), Json(bootstrap)).into_response())
}

pub async fn general_put(
    crate::operations::AdmittedMutation(lease): crate::operations::AdmittedMutation,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LocaleQuery>,
    input: Result<Json<GeneralSettingsInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &state.db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|_| ApiError::bad_request("Dữ liệu Settings không hợp lệ."))?;
    let input = input.validate().map_err(ApiError::bad_request)?;
    let cache_guard = GENERAL_SAVE_CACHE_LOCK.lock().await;
    // Once admitted and started, a disconnect must not cancel the handler
    // between SQLite COMMIT and cache publication. Keep both the admission
    // lease and the serialization guard alive until the commit is published.
    let outcome = tokio::spawn(async move {
        let _lease = lease;
        let _cache_guard = cache_guard;
        let outcome =
            settings::save_general_for_locale(&state.db, query.locale.as_deref(), input, version)
                .await
                .map_err(map_storage_error)?;
        if let SaveOutcome::Saved(ref saved) = outcome {
            state.ui_locale.store(
                crate::i18n::encode_ui_locale(&saved.cms_locale),
                Ordering::Release,
            );
        }
        Ok::<_, ApiError>(outcome)
    })
    .await
    .map_err(|error| {
        eprintln!("General settings save task: {error}");
        ApiError::internal("Không truy cập được dữ liệu.")
    })??;
    match outcome {
        SaveOutcome::Saved(saved) => Ok(settings_response(saved)),
        SaveOutcome::Stale => Err(ApiError::precondition_failed(
            "Dữ liệu đã thay đổi ở một tab khác. Form chưa lưu vẫn được giữ lại.",
        )),
        SaveOutcome::InvalidTimezone => Err(ApiError::bad_request(
            "Múi giờ không có trong danh mục IANA tzdb 2026f.",
        )),
        SaveOutcome::SecondaryTimezoneLocked => Err(ApiError::bad_request(
            "Múi giờ là cấu hình toàn cục và chỉ được thay đổi ở ngôn ngữ mặc định.",
        )),
        SaveOutcome::InvalidCmsLocale => Err(ApiError::bad_request(
            "Ngôn ngữ CMS không có trong danh mục hỗ trợ.",
        )),
        SaveOutcome::SecondaryCmsLocaleLocked => Err(ApiError::bad_request(
            "Ngôn ngữ CMS là cấu hình toàn cục và chỉ được thay đổi ở ngôn ngữ mặc định.",
        )),
        SaveOutcome::InvalidLocale(msg) => Err(ApiError::bad_request(msg)),
    }
}

fn locales_response(status: StatusCode, snapshot: SiteLocalesSnapshot) -> Response {
    (
        status,
        no_store_headers(Some(snapshot.version)),
        Json(snapshot),
    )
        .into_response()
}

fn map_locale_mutation_error(err: LocaleMutationError) -> ApiResult<Response> {
    match err {
        LocaleMutationError::Conflicts(payload) => Ok((
            StatusCode::CONFLICT,
            [(header::CACHE_CONTROL, "no-store")],
            Json(payload),
        )
            .into_response()),
        LocaleMutationError::Api(api_err) => Err(api_err),
    }
}

pub async fn locales_get(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let snapshot = settings::list_locales(&db).await?;
    Ok(locales_response(StatusCode::OK, snapshot))
}

pub async fn locales_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<CreateLocaleInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu tạo ngôn ngữ không hợp lệ."))?;
    match settings::create_locale(&db, version, input).await {
        Ok(snapshot) => Ok(locales_response(StatusCode::CREATED, snapshot)),
        Err(err) => map_locale_mutation_error(err),
    }
}

pub async fn locales_update(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(code): Path<String>,
    input: Result<Json<UpdateLocaleInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu cập nhật ngôn ngữ không hợp lệ."))?;
    match settings::update_locale(&db, &code, version, input).await {
        Ok(snapshot) => Ok(locales_response(StatusCode::OK, snapshot)),
        Err(err) => map_locale_mutation_error(err),
    }
}

pub async fn locales_set_default(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(code): Path<String>,
    body: Bytes,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    if !body.is_empty() {
        let parsed: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|_| ApiError::bad_request("Body của set-default phải rỗng hoặc {}."))?;
        if !parsed.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(ApiError::bad_request(
                "Body của set-default phải rỗng hoặc {}.",
            ));
        }
    }
    match settings::set_default_locale(&db, &code, version).await {
        Ok(snapshot) => Ok(locales_response(StatusCode::OK, snapshot)),
        Err(err) => map_locale_mutation_error(err),
    }
}

fn public_settings_response(settings: PublicSiteSettings) -> Response {
    (no_store_headers(Some(settings.version)), Json(settings)).into_response()
}

pub async fn public_get(State(db): State<SqlitePool>, headers: HeaderMap) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let current = settings::get_public(&db).await?;
    Ok(public_settings_response(current))
}

pub async fn public_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<PublicSiteSettingsInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Dữ liệu cài đặt công khai vượt quá 4 KiB.".into(),
            )
        } else {
            ApiError::bad_request("Dữ liệu cài đặt công khai không hợp lệ.")
        }
    })?;
    let input = input
        .validate()
        .map_err(|msg| ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg.into()))?;
    match settings::save_public(&db, input, version)
        .await
        .map_err(map_storage_error)?
    {
        PublicSaveOutcome::Saved(saved) => Ok(public_settings_response(saved)),
        PublicSaveOutcome::Stale => Err(ApiError::precondition_failed(
            "Cài đặt công khai đã thay đổi ở một tab khác. Form chưa lưu vẫn được giữ lại.",
        )),
        PublicSaveOutcome::ReservedLanguage(msg) => {
            Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, msg.into()))
        }
        PublicSaveOutcome::MultiLocaleConflict => Err(ApiError::conflict(
            "Website đang bật nhiều ngôn ngữ; hãy quản lý ngôn ngữ mặc định tại phần Ngôn ngữ nội dung.",
        )),
        PublicSaveOutcome::SyncFailed(err) => Err(err),
    }
}

fn permalink_settings_response(snapshot: PermalinkSettings) -> Response {
    (no_store_headers(Some(snapshot.version)), Json(snapshot)).into_response()
}

/// SITE-02/C3: always return a server-owned, versioned permalink snapshot.
pub async fn permalinks_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let snapshot = permalinks::get_settings(&db).await?;
    Ok(permalink_settings_response(snapshot))
}

/// Persist format and bases atomically alongside canonical/redirect claims.
/// The storage writer owns collision checks and the 50,000-claim limit.
pub async fn permalinks_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<PermalinkSettingsInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|error| {
        if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Cấu hình permalink vượt quá 16 KiB.".into(),
            )
        } else {
            ApiError::bad_request("Cấu hình permalink không hợp lệ.")
        }
    })?;
    match permalinks::put_settings(&db, expected, &input).await {
        Ok(saved) => Ok(permalink_settings_response(saved)),
        Err(PermalinkPutError::Conflicts(payload)) => Ok((
            StatusCode::CONFLICT,
            [(header::CACHE_CONTROL, "no-store")],
            Json(payload),
        )
            .into_response()),
        Err(PermalinkPutError::Api(err)) => Err(err),
    }
}

fn reading_settings_response(settings: ReadingSettings) -> Response {
    (no_store_headers(Some(settings.version)), Json(settings)).into_response()
}

pub async fn reading_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<LocaleQuery>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    Ok(reading_settings_response(
        settings::get_reading_for_locale(&db, query.locale.as_deref()).await?,
    ))
}

pub async fn reading_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<LocaleQuery>,
    input: Result<Json<ReadingSettingsInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let expected = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|_| ApiError::bad_request("Reading Settings không hợp lệ."))?;
    let saved =
        settings::save_reading_for_locale(&db, query.locale.as_deref(), input, expected).await?;
    Ok(reading_settings_response(saved))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeOptionsPutInput {
    pub values: BTreeMap<String, serde_json::Value>,
}

fn theme_options_response(snapshot: ThemeOptionsSnapshot) -> Response {
    (no_store_headers(Some(snapshot.version)), Json(snapshot)).into_response()
}

pub async fn theme_options_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(theme_id): Path<String>,
    Query(query): Query<LocaleQuery>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let Some(snapshot) =
        settings::get_theme_options_for_locale(&db, &theme_id, query.locale.as_deref()).await?
    else {
        return Err(ApiError::not_found("Không tìm thấy theme."));
    };
    Ok(theme_options_response(snapshot))
}

pub async fn theme_options_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(theme_id): Path<String>,
    Query(query): Query<LocaleQuery>,
    input: Result<Json<ThemeOptionsPutInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) =
        input.map_err(|_| ApiError::bad_request("Dữ liệu theme options không hợp lệ."))?;
    match settings::put_theme_options_for_locale(
        &db,
        &theme_id,
        version,
        input.values,
        query.locale.as_deref(),
    )
    .await?
    {
        ThemeOptionsOutcome::Saved(saved) => Ok(theme_options_response(saved)),
        ThemeOptionsOutcome::NotFound => Err(ApiError::not_found("Không tìm thấy theme.")),
        ThemeOptionsOutcome::Stale => Err(ApiError::precondition_failed(
            "Theme options đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        )),
    }
}

fn site_options_response(snapshot: GlobalSiteOptionsSnapshot) -> Response {
    (no_store_headers(Some(snapshot.version)), Json(snapshot)).into_response()
}

pub async fn site_options_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let snapshot = settings::get_global_site_options(&db).await?;
    Ok(site_options_response(snapshot))
}

pub async fn site_options_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    input: Result<Json<ThemeOptionsPutInput>, JsonRejection>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &db).await?;
    let version = parse_if_match(&headers)?;
    let Json(input) = input.map_err(|err| {
        if err.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Dữ liệu tùy chọn toàn trang vượt quá 64 KiB.".into(),
            )
        } else {
            ApiError::bad_request("Dữ liệu tùy chọn toàn trang không hợp lệ.")
        }
    })?;
    match settings::put_global_site_options(&db, version, input.values).await? {
        GlobalSiteOptionsOutcome::Saved(saved) => Ok(site_options_response(saved)),
        GlobalSiteOptionsOutcome::Stale => Err(ApiError::precondition_failed(
            "Tùy chọn toàn trang đã thay đổi ở phiên khác. Tải lại trước khi lưu.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn etag_rejects_missing_weak_and_malformed_versions() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            parse_if_match(&headers).unwrap_err().0,
            StatusCode::PRECONDITION_REQUIRED
        );
        for value in ["W/\"v1\"", "v1", "\"v0\"", "\"v-1\"", "\"v1\",\"v2\""] {
            headers.insert(header::IF_MATCH, HeaderValue::from_str(value).unwrap());
            assert_eq!(
                parse_if_match(&headers).unwrap_err().0,
                StatusCode::BAD_REQUEST
            );
        }
        headers.insert(header::IF_MATCH, HeaderValue::from_static("\"v42\""));
        assert_eq!(parse_if_match(&headers).unwrap(), 42);
    }
}
