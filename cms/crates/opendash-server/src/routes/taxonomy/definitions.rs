use super::*;

fn definition_response(status: StatusCode, definition: TaxonomyDefinition) -> Response {
    (
        status,
        [
            (header::ETAG, format!("\"v{}\"", definition.version)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(definition),
    )
        .into_response()
}

pub async fn taxonomy_definitions_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(taxonomy::list_definitions(&db).await?),
    )
        .into_response())
}

pub async fn taxonomy_definitions_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let definition = taxonomy::get_definition(&db, &key)
        .await?
        .ok_or_else(|| ApiError::not_found("Taxonomy không tồn tại."))?;
    Ok(definition_response(StatusCode::OK, definition))
}

/// Separate authenticated projection of Term Meta groups and defaults for New Term UI.
pub async fn taxonomy_definitions_effective_editor_config(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let role = if actor.user.role == Role::Admin {
        FieldRole::Admin
    } else {
        FieldRole::Editor
    };
    let config = taxonomy::new_term_editor_config(&db, &key, role).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(config)).into_response())
}

pub async fn taxonomy_definitions_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<TaxonomyDefinitionInput>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let created = taxonomy::create_definition(&db, &actor, &input).await?;
    Ok(definition_response(StatusCode::CREATED, created))
}

pub async fn taxonomy_definitions_patch(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
    payload: Result<Json<TaxonomyDefinitionPatch>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let expected = expected_version(&headers)?;
    let input = parse_body(payload)?;
    let updated = taxonomy::patch_definition(&db, &actor, &key, expected, &input).await?;
    Ok(definition_response(StatusCode::OK, updated))
}

pub async fn taxonomy_definitions_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(key): Path<String>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    if !valid_content_type_key(&key) {
        return Err(ApiError::bad_request("Khóa taxonomy không hợp lệ."));
    }
    let expected = expected_version(&headers)?;
    taxonomy::delete_definition(&db, &actor, &key, expected).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}

pub async fn taxonomy_eligibility_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let rows =
        crate::storage::content::list_content_type_taxonomy_eligibility(&db, &content_type_key)
            .await?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(rows),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyEligibilityPut {
    taxonomies: Vec<String>,
    expected_version: i64,
}

pub async fn taxonomy_eligibility_put(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
    payload: Result<Json<TaxonomyEligibilityPut>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let etag_version = crate::routes::settings::parse_if_match(&headers)?;
    if input.expected_version != etag_version {
        return Err(ApiError::bad_request(
            "expected_version không khớp If-Match.",
        ));
    }
    let updated = crate::storage::content::put_site_taxonomy_eligibility(
        &db,
        &actor,
        &content_type_key,
        etag_version,
        &input.taxonomies,
    )
    .await?;
    Ok((
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::ETAG, format!("\"v{}\"", updated.version)),
        ],
        Json(updated),
    )
        .into_response())
}

pub async fn taxonomy_eligibility_effective(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if !valid_content_type_key(&content_type_key) {
        return Err(ApiError::bad_request("Khóa CPT không hợp lệ."));
    }
    let keys = crate::storage::content::load_effective_taxonomies(&db, &content_type_key).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(keys)).into_response())
}

/// Admin reads the entire persisted selection before replacing a site
/// override. Returning only effective keys would silently drop dormant keys.
pub async fn taxonomy_eligibility_configured(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(content_type_key): Path<String>,
) -> ApiResult<Response> {
    require_admin(&headers, &db).await?;
    let configured =
        crate::storage::content::load_configured_taxonomies(&db, &content_type_key).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(configured)).into_response())
}
