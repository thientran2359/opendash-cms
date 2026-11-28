use super::*;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyPagination {
    kind: Option<String>,
    locale: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyTermResolveQuery {
    ids: String,
}

pub(super) fn parse_term_resolve_ids(raw: &str) -> ApiResult<Vec<i64>> {
    if raw.is_empty() || raw.len() > 2000 {
        return Err(ApiError::bad_request("Tối đa 100 ID term mỗi lần tra cứu."));
    }
    let mut ids = Vec::new();
    let mut unique = std::collections::HashSet::new();
    for raw_id in raw.split(',') {
        if ids.len() == 100
            || raw_id.is_empty()
            || raw_id.starts_with('0')
            || !raw_id.bytes().all(|digit| digit.is_ascii_digit())
        {
            return Err(ApiError::bad_request(
                "Danh sách ID term không hợp lệ hoặc vượt 100 ID.",
            ));
        }
        let id = raw_id
            .parse::<i64>()
            .map_err(|_| ApiError::bad_request("ID term không hợp lệ."))?;
        if id <= 0 || !unique.insert(id) {
            return Err(ApiError::bad_request(
                "Danh sách ID term không hợp lệ hoặc trùng.",
            ));
        }
        ids.push(id);
    }
    Ok(ids)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTaxonomyTerm {
    kind: String,
    name: String,
    slug: String,
    parent_id: Option<i64>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    sort_order: Option<i32>,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    source_term_id: Option<i64>,
    #[serde(default)]
    featured_image_media_id: Option<String>,
    #[serde(default)]
    values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    schema_token: Option<String>,
    #[serde(default)]
    seo: Option<TermSeoInput>,
}

fn present_optional_parent<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(Some)
}

fn present_optional_media_id<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok(Some(value))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchTaxonomyTerm {
    pub(super) name: Option<String>,
    pub(super) slug: Option<String>,
    #[serde(default, deserialize_with = "present_optional_parent")]
    pub(super) parent_id: Option<Option<i64>>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) sort_order: Option<i32>,
    #[serde(default, deserialize_with = "present_optional_media_id")]
    pub(super) featured_image_media_id: Option<Option<String>>,
    #[serde(default)]
    pub(super) values: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub(super) extensions: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub(super) schema_token: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub(super) seo: PatchField<TermSeoInput>,
}

fn term_response(status: StatusCode, term: taxonomy::TaxonomyTerm) -> Response {
    (
        status,
        [
            (header::ETAG, format!("\"v{}\"", term.version)),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(term),
    )
        .into_response()
}

pub async fn taxonomy_list(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<TaxonomyPagination>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    if let Some(key) = query.kind.as_deref()
        && (!valid_content_type_key(key) || taxonomy::get_definition(&db, key).await?.is_none())
    {
        return Err(ApiError::bad_request("Taxonomy không tồn tại."));
    }
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) || offset < 0 {
        return Err(ApiError::bad_request("Phân trang taxonomy không hợp lệ."));
    }
    let locale_filter = match query.locale.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim().to_ascii_lowercase();
            if trimmed.is_empty() || trimmed.len() > 35 || trimmed.chars().any(char::is_control) {
                return Err(ApiError::bad_request("Tham số locale không hợp lệ."));
            }
            if trimmed == "all" {
                None
            } else {
                Some(trimmed)
            }
        }
        None => None,
    };
    let page = taxonomy::list_terms_for_locale(
        &db,
        query.kind.as_deref(),
        locale_filter.as_deref(),
        limit,
        offset,
    )
    .await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

pub async fn taxonomy_terms_resolve(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Query(query): Query<TaxonomyTermResolveQuery>,
) -> ApiResult<Response> {
    require_session(&headers, &db).await?;
    let ids = parse_term_resolve_ids(&query.ids)?;
    let result = taxonomy::resolve_term_labels(&db, &ids).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}

pub async fn taxonomy_get(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_id(id)?;
    let result = taxonomy::get_term(&db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term."))?;
    let result = taxonomy::term_with_effective_extensions(
        &db,
        result,
        if actor.user.role == Role::Admin {
            "admin"
        } else {
            "editor"
        },
    )
    .await?;
    Ok(term_response(StatusCode::OK, result))
}

pub async fn taxonomy_create(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    payload: Result<Json<NewTaxonomyTerm>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    let input = parse_body(payload)?;
    let featured_image_media_id = input
        .featured_image_media_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let created = taxonomy::create_term_full(
        &db,
        &actor,
        &input.kind,
        &input.name,
        &input.slug,
        input.parent_id,
        input.description.as_deref(),
        input.sort_order,
        input.locale.as_deref(),
        input.source_term_id,
        featured_image_media_id,
        input.values.as_ref(),
        input.schema_token.as_deref(),
        input.seo.as_ref(),
    )
    .await?;
    let role = if actor.user.role == Role::Admin {
        "admin"
    } else {
        "editor"
    };
    let created = taxonomy::term_with_effective_extensions(&db, created, role).await?;
    Ok(term_response(StatusCode::CREATED, created))
}

pub async fn taxonomy_patch(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    payload: Result<Json<PatchTaxonomyTerm>, JsonRejection>,
) -> ApiResult<Response> {
    let actor = require_session_with_csrf(&headers, &db).await?;
    valid_id(id)?;
    let version = expected_version(&headers)?;
    let input = parse_body(payload)?;
    let merged_values = input.values.as_ref().or(input.extensions.as_ref());
    if input.name.is_none()
        && input.slug.is_none()
        && input.parent_id.is_none()
        && input.description.is_none()
        && input.sort_order.is_none()
        && input.featured_image_media_id.is_none()
        && merged_values.is_none()
        && input.seo.is_omitted()
    {
        return Err(ApiError::bad_request(
            "Cần name, slug, parent_id, description, sort_order, featured_image_media_id, values hoặc seo để cập nhật.",
        ));
    }
    let updated = taxonomy::patch_term_full(
        &db,
        &actor,
        id,
        version,
        input.name.as_deref(),
        input.slug.as_deref(),
        input.parent_id,
        input.description.as_deref(),
        input.sort_order,
        input
            .featured_image_media_id
            .as_ref()
            .map(|opt| opt.as_deref()),
        merged_values,
        input.schema_token.as_deref(),
        &input.seo,
    )
    .await?;
    let role = if actor.user.role == Role::Admin {
        "admin"
    } else {
        "editor"
    };
    let updated = taxonomy::term_with_effective_extensions(&db, updated, role).await?;
    Ok(term_response(StatusCode::OK, updated))
}

/// Separate authenticated projection of readable field definitions & values for term Edit.
pub async fn taxonomy_effective_editor_config(
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_session(&headers, &db).await?;
    valid_id(id)?;
    let term = taxonomy::get_term(&db, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy term."))?;
    let role = if actor.user.role == Role::Admin {
        FieldRole::Admin
    } else {
        FieldRole::Editor
    };
    let config = taxonomy::effective_term_editor_config(&db, id, &term.kind, role).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(config)).into_response())
}

pub async fn taxonomy_delete(
    crate::operations::AdmittedMutation(_lease): crate::operations::AdmittedMutation,
    State(db): State<SqlitePool>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let actor = require_admin_with_csrf(&headers, &db).await?;
    valid_id(id)?;
    let version = expected_version(&headers)?;
    taxonomy::delete_term(&db, &actor, id, version).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}
