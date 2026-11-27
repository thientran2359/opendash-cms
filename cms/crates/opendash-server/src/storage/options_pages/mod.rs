//! Persistent Custom Options Pages and attached FieldGroup values (`owner_kind = 'options_page'`).

mod public;
mod values;
pub use public::*;
pub use values::*;

use opendash_core::{
    content::{
        CustomOptionsPageDeclaration, CustomOptionsPageDefinition, CustomOptionsPageInput,
        FieldRole, MAX_CUSTOM_OPTIONS_PAGES_PER_SITE, OptionsPageParentMenu,
        valid_custom_options_page_key,
    },
    registry::{DeclarationState, ProviderStatus, ResourceOwner},
};
use sqlx::{SqliteConnection, SqlitePool};

use crate::error::{ApiError, ApiResult};
use crate::storage::content::{
    begin_content_write_tx, map_input_validation_error, parse_declaration_state, parse_owner,
    parse_provider_status, table_exists_in_tx,
};

type PageRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i32>,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
);

fn parse_parent_menu(raw: &str) -> ApiResult<OptionsPageParentMenu> {
    OptionsPageParentMenu::from_db(raw).ok_or_else(|| {
        ApiError::internal("parent_menu của custom_options_pages không hợp lệ.")
    })
}

fn parent_menu_db(menu: OptionsPageParentMenu) -> &'static str {
    menu.as_str()
}

fn parse_role(raw: &str) -> ApiResult<FieldRole> {
    match raw {
        "admin" => Ok(FieldRole::Admin),
        "editor" => Ok(FieldRole::Editor),
        _ => Err(ApiError::internal(
            "min_edit_role của custom_options_pages không hợp lệ.",
        )),
    }
}

fn role_db(role: FieldRole) -> &'static str {
    match role {
        FieldRole::Admin => "admin",
        FieldRole::Editor => "editor",
    }
}

fn row_to_def(row: PageRow) -> ApiResult<CustomOptionsPageDefinition> {
    let (
        page_key,
        title,
        menu_label,
        description,
        parent_str,
        menu_icon,
        pos_opt,
        role_str,
        owner_kind,
        owner_id,
        status_str,
        decl_str,
        version,
    ) = row;
    Ok(CustomOptionsPageDefinition {
        page_key,
        owner: parse_owner(&owner_kind, owner_id)?,
        title,
        menu_label,
        description,
        menu_icon,
        menu_position: pos_opt.unwrap_or(50),
        parent_menu: parse_parent_menu(&parent_str)?,
        min_edit_role: parse_role(&role_str)?,
        provider_status: parse_provider_status(&status_str)?,
        declaration_state: parse_declaration_state(&decl_str)?,
        version,
    })
}

pub(in crate::storage) async fn get_options_page_in_conn(
    conn: &mut SqliteConnection,
    page_key: &str,
) -> ApiResult<Option<CustomOptionsPageDefinition>> {
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        return Ok(None);
    }
    let row: Option<PageRow> = sqlx::query_as(
        "SELECT page_key, title, menu_title, description, parent_menu, icon_key, position,
                min_edit_role, owner_kind, owner_id, provider_status, declaration_state, version
           FROM custom_options_pages WHERE page_key = ?",
    )
    .bind(page_key)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(row_to_def).transpose()
}

pub async fn list_options_pages(db: &SqlitePool) -> ApiResult<Vec<CustomOptionsPageDefinition>> {
    let mut conn = db.acquire().await?;
    if !table_exists_in_tx(&mut conn, "custom_options_pages").await? {
        return Ok(Vec::new());
    }
    let rows: Vec<PageRow> = sqlx::query_as(
        "SELECT page_key, title, menu_title, description, parent_menu, icon_key, position,
                min_edit_role, owner_kind, owner_id, provider_status, declaration_state, version
           FROM custom_options_pages
          ORDER BY COALESCE(position, 50) ASC, page_key ASC",
    )
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter().map(row_to_def).collect()
}

pub async fn get_options_page(
    db: &SqlitePool,
    page_key: &str,
) -> ApiResult<Option<CustomOptionsPageDefinition>> {
    valid_custom_options_page_key(page_key).map_err(ApiError::bad_request)?;
    let mut conn = db.acquire().await?;
    get_options_page_in_conn(&mut conn, page_key).await
}

pub async fn create_user_options_page(
    db: &SqlitePool,
    input: &CustomOptionsPageInput,
) -> ApiResult<CustomOptionsPageDefinition> {
    let mut normalized = input.clone();
    normalized
        .validate_and_normalize()
        .map_err(map_input_validation_error)?;
    let mut tx = begin_content_write_tx(db).await?;
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM custom_options_pages")
        .fetch_one(&mut *tx)
        .await?;
    if total as usize >= MAX_CUSTOM_OPTIONS_PAGES_PER_SITE {
        return Err(ApiError::conflict(
            "Website đạt giới hạn tối đa 64 trang tùy chọn.",
        ));
    }
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM custom_options_pages WHERE page_key = ?")
            .bind(&normalized.page_key)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_some() {
        return Err(ApiError::conflict("page_key của trang tùy chọn đã tồn tại."));
    }
    sqlx::query(
        "INSERT INTO custom_options_pages
         (page_key, title, menu_title, description, parent_menu, icon_key, position,
          min_edit_role, owner_kind, owner_id, provider_status, declaration_state, version)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'user', NULL, 'active', 'declared', 1)",
    )
    .bind(&normalized.page_key)
    .bind(&normalized.title)
    .bind(&normalized.menu_label)
    .bind(&normalized.description)
    .bind(parent_menu_db(normalized.parent_menu))
    .bind(&normalized.menu_icon)
    .bind(normalized.menu_position)
    .bind(role_db(normalized.min_edit_role))
    .execute(&mut *tx)
    .await?;

    let created = get_options_page_in_conn(&mut tx, &normalized.page_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể đọc trang tùy chọn vừa tạo."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(created)
}

pub async fn update_user_options_page(
    db: &SqlitePool,
    page_key: &str,
    expected_version: i64,
    input: &CustomOptionsPageInput,
) -> ApiResult<CustomOptionsPageDefinition> {
    if input.page_key != page_key {
        return Err(ApiError::bad_request(
            "page_key trong body không khớp với đường dẫn URL.",
        ));
    }
    let mut normalized = input.clone();
    normalized
        .validate_and_normalize()
        .map_err(map_input_validation_error)?;
    let mut tx = begin_content_write_tx(db).await?;
    let current = get_options_page_in_conn(&mut tx, page_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy trang tùy chọn."))?;
    if current.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể sửa trang tùy chọn thuộc quyền sở hữu của User.",
        ));
    }
    if current.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản trang tùy chọn đã thay đổi. Vui lòng tải lại.",
        ));
    }
    sqlx::query(
        "UPDATE custom_options_pages
            SET title = ?, menu_title = ?, description = ?, parent_menu = ?, icon_key = ?,
                position = ?, min_edit_role = ?, version = version + 1,
                updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
          WHERE page_key = ? AND version = ?",
    )
    .bind(&normalized.title)
    .bind(&normalized.menu_label)
    .bind(&normalized.description)
    .bind(parent_menu_db(normalized.parent_menu))
    .bind(&normalized.menu_icon)
    .bind(normalized.menu_position)
    .bind(role_db(normalized.min_edit_role))
    .bind(page_key)
    .bind(expected_version)
    .execute(&mut *tx)
    .await?;

    let updated = get_options_page_in_conn(&mut tx, page_key)
        .await?
        .ok_or_else(|| ApiError::internal("Không thể đọc trang tùy chọn sau cập nhật."))?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(updated)
}

pub async fn delete_user_options_page(
    db: &SqlitePool,
    page_key: &str,
    expected_version: i64,
    purge: bool,
) -> ApiResult<()> {
    valid_custom_options_page_key(page_key).map_err(ApiError::bad_request)?;
    let mut tx = begin_content_write_tx(db).await?;
    let current = get_options_page_in_conn(&mut tx, page_key)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy trang tùy chọn."))?;
    if current.owner != ResourceOwner::User {
        return Err(ApiError::forbidden(
            "Chỉ có thể xóa trang tùy chọn thuộc quyền sở hữu của User.",
        ));
    }
    if current.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Phiên bản trang tùy chọn đã thay đổi. Vui lòng tải lại.",
        ));
    }
    let page_ref = format!("custom:{page_key}");
    let has_data: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM theme_option_values WHERE owner_kind = 'options_page' AND owner_id = ?)
             OR EXISTS(SELECT 1 FROM theme_option_locale_values WHERE owner_kind = 'options_page' AND owner_id = ?)",
    )
    .bind(&page_ref)
    .bind(&page_ref)
    .fetch_one(&mut *tx)
    .await?;
    let has_groups: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM content_field_group_targets
           WHERE target_kind = 'options_page' AND target_key = ? AND association_state = 'declared')",
    )
    .bind(&page_ref)
    .fetch_one(&mut *tx)
    .await?;
    if !purge && (has_data || has_groups) {
        return Err(ApiError::conflict(
            "cannot_delete_options_page_with_data: Trang tùy chọn còn nhóm trường hoặc dữ liệu; hãy gỡ gắn hoặc thêm ?purge=true.",
        ));
    }
    if purge {
        for tbl in [
            "theme_option_values",
            "theme_option_versions",
            "theme_option_locale_values",
            "theme_option_locale_versions",
            "theme_option_media_refs",
        ] {
            let sql = format!("DELETE FROM {tbl} WHERE owner_kind = 'options_page' AND owner_id = ?");
            sqlx::query(sqlx::AssertSqlSafe(sql)).bind(&page_ref).execute(&mut *tx).await?;
        }
        sqlx::query(
            "DELETE FROM content_field_group_targets
              WHERE target_kind = 'options_page' AND target_key = ?",
        )
        .bind(&page_ref)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM custom_options_pages WHERE page_key = ? AND version = ?")
        .bind(page_key)
        .bind(expected_version)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(())
}

pub async fn sync_provider_options_pages_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    pages: &[CustomOptionsPageDeclaration],
) -> ApiResult<()> {
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for page in pages {
        let mut norm = page.clone();
        norm.validate_and_normalize()
            .map_err(map_input_validation_error)?;
        if !seen.insert(norm.page_key.clone()) {
            return Err(ApiError::bad_request(
                "Khai báo options_pages chứa page_key trùng lặp.",
            ));
        }
        if let Some(existing) = get_options_page_in_conn(conn, &norm.page_key).await? {
            let expected_owner = parse_owner(owner_kind, Some(owner_id.to_owned()))?;
            if existing.owner != expected_owner {
                return Err(ApiError::conflict(
                    "page_key của trang tùy chọn thuộc chủ sở hữu khác (409).",
                ));
            }
            sqlx::query(
                "UPDATE custom_options_pages
                    SET title = ?, menu_title = ?, description = ?, parent_menu = ?, icon_key = ?,
                        position = ?, min_edit_role = ?, provider_status = 'active',
                        declaration_state = 'declared', version = version + 1,
                        updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                  WHERE page_key = ?",
            )
            .bind(&norm.title)
            .bind(&norm.menu_label)
            .bind(&norm.description)
            .bind(parent_menu_db(norm.parent_menu))
            .bind(&norm.menu_icon)
            .bind(norm.menu_position)
            .bind(role_db(norm.min_edit_role))
            .bind(&norm.page_key)
            .execute(&mut *conn)
            .await?;
        } else {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM custom_options_pages")
                .fetch_one(&mut *conn)
                .await?;
            if total as usize >= MAX_CUSTOM_OPTIONS_PAGES_PER_SITE {
                return Err(ApiError::conflict(
                    "Website đạt giới hạn tối đa 64 trang tùy chọn.",
                ));
            }
            sqlx::query(
                "INSERT INTO custom_options_pages
                 (page_key, title, menu_title, description, parent_menu, icon_key, position,
                  min_edit_role, owner_kind, owner_id, provider_status, declaration_state, version)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', 'declared', 1)",
            )
            .bind(&norm.page_key)
            .bind(&norm.title)
            .bind(&norm.menu_label)
            .bind(&norm.description)
            .bind(parent_menu_db(norm.parent_menu))
            .bind(&norm.menu_icon)
            .bind(norm.menu_position)
            .bind(role_db(norm.min_edit_role))
            .bind(owner_kind)
            .bind(owner_id)
            .execute(&mut *conn)
            .await?;
        }
    }
    let existing_keys: Vec<String> = sqlx::query_scalar(
        "SELECT page_key FROM custom_options_pages WHERE owner_kind = ? AND owner_id = ?",
    )
    .bind(owner_kind)
    .bind(owner_id)
    .fetch_all(&mut *conn)
    .await?;
    for key in existing_keys {
        if !seen.contains(&key) {
            sqlx::query(
                "UPDATE custom_options_pages
                    SET declaration_state = 'withdrawn', provider_status = 'active',
                        version = version + 1,
                        updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                  WHERE page_key = ? AND (declaration_state != 'withdrawn' OR provider_status != 'active')",
            )
            .bind(&key)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

pub async fn deactivate_other_theme_options_pages_in_tx(
    conn: &mut SqliteConnection,
    active_theme_id: &str,
) -> ApiResult<()> {
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        return Ok(());
    }
    sqlx::query(
        "UPDATE custom_options_pages
            SET provider_status = 'provider_inactive',
                updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
          WHERE owner_kind = 'theme' AND owner_id != ? AND provider_status != 'provider_inactive'",
    )
    .bind(active_theme_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn set_provider_options_pages_status_in_tx(
    conn: &mut SqliteConnection,
    owner_kind: &str,
    owner_id: &str,
    active: bool,
) -> ApiResult<()> {
    if !table_exists_in_tx(conn, "custom_options_pages").await? {
        return Ok(());
    }
    if active {
        sqlx::query(
            "UPDATE custom_options_pages
                SET provider_status = 'active', updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
              WHERE owner_kind = ? AND owner_id = ? AND declaration_state = 'declared'
                AND provider_status != 'active'",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .execute(&mut *conn)
        .await?;
    } else {
        sqlx::query(
            "UPDATE custom_options_pages
                SET provider_status = 'provider_inactive', updated_at = (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
              WHERE owner_kind = ? AND owner_id = ? AND provider_status != 'provider_inactive'",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
