use super::*;

pub(in crate::storage) type DefinitionRow = (
    String,
    String,
    i64,
    i64,
    i64,
    String,
    Option<String>,
    String,
    String,
);

/// Reconcile provider-owned custom taxonomy declarations in the same writer
/// transaction as its CPTs, settings and permalink claims. Withdrawal and
/// disable are state transitions: they never delete terms or memberships.
pub(in crate::storage) async fn ensure_non_reserved_taxonomy_key_in_tx(
    conn: &mut SqliteConnection,
    key: &str,
) -> ApiResult<()> {
    if crate::seo::is_reserved_system_root(key) || key == "posts" {
        return Err(ApiError::conflict(
            "Khóa taxonomy trùng với tiền tố hệ thống được bảo vệ.",
        ));
    }
    let cpt_conflict: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM content_types WHERE key = ? LIMIT 1")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?;
    if cpt_conflict.is_some() {
        return Err(ApiError::conflict(
            "Khóa taxonomy trùng với khóa loại nội dung đang tồn tại.",
        ));
    }
    Ok(())
}

pub(in crate::storage) fn normalized_name(value: &str) -> ApiResult<&str> {
    let trimmed = value.trim();
    if !(1..=120).contains(&trimmed.chars().count()) {
        return Err(ApiError::bad_request(
            "Tên term phải dài từ 1 đến 120 ký tự.",
        ));
    }
    Ok(trimmed)
}

pub(in crate::storage) fn mutation_error(error: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(ref database_error) = error {
        if database_error.is_unique_violation() {
            return ApiError::conflict("Slug đã tồn tại trong loại taxonomy.");
        }
        if let Some(code) = database_error.code() {
            if code.parse::<u32>().is_ok_and(|n| matches!(n & 0xff, 5 | 6))
                || code.starts_with("SQLITE_BUSY")
                || code.starts_with("SQLITE_LOCKED")
            {
                return ApiError::service_unavailable(
                    "Dữ liệu đang được cập nhật, vui lòng thử lại.",
                );
            }
            if code.parse::<u32>().is_ok_and(|n| n & 0xff == 19)
                || code.starts_with("SQLITE_CONSTRAINT")
            {
                return ApiError::conflict("Dữ liệu taxonomy vi phạm ràng buộc.");
            }
        }
    }
    if matches!(error, sqlx::Error::PoolTimedOut) {
        return ApiError::service_unavailable("Dữ liệu đang được cập nhật, vui lòng thử lại.");
    }
    ApiError::from(error)
}

pub(in crate::storage) async fn validate_featured_image_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    raw: &str,
) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.len() != 64
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ApiError::bad_request("ID ảnh đại diện không hợp lệ."));
    }
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT id FROM media_assets WHERE id = ? AND kind = 'image' AND lifecycle_state = 'active'",
    )
    .bind(trimmed)
    .fetch_optional(&mut **tx)
    .await
    .map_err(mutation_error)?;
    exists.ok_or_else(|| {
        ApiError::bad_request("Ảnh đại diện không tồn tại hoặc không phải ảnh đang hoạt động.")
    })
}

/// Height includes the selected node (a leaf has height one). The recursion
/// is capped so corrupted cyclic data cannot keep the CTE running forever.
pub(in crate::storage) async fn subtree_height(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> ApiResult<i64> {
    let height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE subtree(id, depth) AS (
             SELECT id, 1 FROM taxonomy_terms WHERE id = ?
             UNION ALL
             SELECT child.id, subtree.depth + 1
               FROM taxonomy_terms child JOIN subtree ON child.parent_id = subtree.id
              WHERE subtree.depth <= 8
         )
         SELECT COALESCE(MAX(depth), 0) FROM subtree",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(mutation_error)?;
    Ok(height)
}
