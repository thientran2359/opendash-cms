//! SITE-01 site-owned menus, hierarchical items, slot assignments, and
//! canonical MenuViewModel rendering.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use url::Url;

use crate::{
    error::{ApiError, ApiResult},
    public_context::{CanonicalUrlBuilder, TaxonomyKind},
};

pub const MAX_MENU_ITEMS: usize = 200;
pub const MAX_MENU_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuTargetKind {
    Entry,
    Term,
    External,
}

impl MenuTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Term => "term",
            Self::External => "external",
        }
    }

    pub fn parse(raw: &str) -> ApiResult<Self> {
        match raw {
            "entry" => Ok(Self::Entry),
            "term" => Ok(Self::Term),
            "external" => Ok(Self::External),
            _ => Err(ApiError::bad_request("target_kind menu không hợp lệ.")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMenuInput {
    pub menu_key: String,
    pub title: String,
    pub origin_provenance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItemInput {
    /// Optional client/existing stable ID (`id` or `client_key` for tree wiring).
    /// When `id` is `Some(existing_id)`, preserves the stable item ID if it belongs to this menu.
    pub id: Option<i64>,
    /// Optional temporary key for referencing a newly created parent in the same batch.
    pub client_key: Option<String>,
    pub parent_id: Option<i64>,
    pub parent_client_key: Option<String>,
    pub position: i64,
    pub label: String,
    pub target_kind: MenuTargetKind,
    pub target_entry_id: Option<i64>,
    pub target_term_id: Option<i64>,
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuRecord {
    pub id: i64,
    pub menu_key: String,
    pub title: String,
    pub version: i64,
    pub origin_provenance: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuItemRecord {
    pub id: i64,
    pub menu_id: i64,
    pub parent_id: Option<i64>,
    pub position: i64,
    pub label: String,
    pub target_kind: String,
    pub target_entry_id: Option<i64>,
    pub target_term_id: Option<i64>,
    pub external_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuWithItems {
    pub menu: MenuRecord,
    pub items: Vec<MenuItemRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuDiagnostic {
    pub item_id: i64,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuItemView {
    pub id: i64,
    pub label: String,
    pub url: String,
    pub children: Vec<MenuItemView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuViewModel {
    pub menu_id: i64,
    pub menu_key: String,
    pub title: String,
    pub items: Vec<MenuItemView>,
    pub diagnostics: Vec<MenuDiagnostic>,
}

pub fn validate_menu_key(raw: &str) -> ApiResult<()> {
    let valid = (1..=64).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !raw.starts_with('-')
        && !raw.ends_with('-');
    if !valid {
        return Err(ApiError::bad_request("menu_key không hợp lệ."));
    }
    Ok(())
}

pub fn validate_external_url(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return Err(ApiError::bad_request("URL menu không hợp lệ hoặc quá dài."));
    }
    if trimmed.starts_with("//") || trimmed.chars().any(|c| c.is_control()) {
        return Err(ApiError::bad_request(
            "URL menu không được dùng protocol-relative hoặc ký tự điều khiển.",
        ));
    }
    let parsed = Url::parse(trimmed)
        .map_err(|_| ApiError::bad_request("URL menu phải là HTTP(S) hợp lệ."))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ApiError::bad_request("URL menu chỉ chấp nhận http hoặc https."));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::bad_request("URL menu không được chứa credentials."));
    }
    if parsed.host_str().is_none() {
        return Err(ApiError::bad_request("URL menu thiếu host."));
    }
    Ok(trimmed.to_string())
}

fn validate_label(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 || trimmed.chars().any(|c| c.is_control())
    {
        return Err(ApiError::bad_request("Nhãn menu item không hợp lệ."));
    }
    Ok(trimmed.to_string())
}

pub async fn create_menu_in_tx(
    conn: &mut SqliteConnection,
    input: &CreateMenuInput,
) -> ApiResult<MenuRecord> {
    validate_menu_key(&input.menu_key)?;
    let title = validate_label(&input.title)?;
    if let Some(ref origin) = input.origin_provenance {
        if origin.trim().is_empty() || origin.len() > 256 {
            return Err(ApiError::bad_request("origin_provenance không hợp lệ."));
        }
        let dup_origin: Option<i64> =
            sqlx::query_scalar("SELECT id FROM site_menus WHERE origin_provenance = ?")
                .bind(origin)
                .fetch_optional(&mut *conn)
                .await?;
        if dup_origin.is_some() {
            return Err(ApiError::conflict("origin_provenance menu đã tồn tại."));
        }
    }
    let dup_key: Option<i64> =
        sqlx::query_scalar("SELECT id FROM site_menus WHERE menu_key = ?")
            .bind(&input.menu_key)
            .fetch_optional(&mut *conn)
            .await?;
    if dup_key.is_some() {
        return Err(ApiError::conflict("menu_key đã tồn tại."));
    }

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO site_menus (menu_key, title, version, origin_provenance)
         VALUES (?, ?, 1, ?)
         RETURNING id",
    )
    .bind(&input.menu_key)
    .bind(&title)
    .bind(input.origin_provenance.as_deref())
    .fetch_one(&mut *conn)
    .await?;

    get_menu_record_in_tx(conn, id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được menu vừa tạo."))
}

async fn get_menu_record_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
) -> ApiResult<Option<MenuRecord>> {
    let row: Option<(i64, String, String, i64, Option<String>, String)> = sqlx::query_as(
        "SELECT id, menu_key, title, version, origin_provenance, updated_at
           FROM site_menus
          WHERE id = ?",
    )
    .bind(menu_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| MenuRecord {
        id: r.0,
        menu_key: r.1,
        title: r.2,
        version: r.3,
        origin_provenance: r.4,
        updated_at: r.5,
    }))
}

pub async fn get_menu_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
) -> ApiResult<Option<MenuWithItems>> {
    let Some(menu) = get_menu_record_in_tx(conn, menu_id).await? else {
        return Ok(None);
    };
    let rows: Vec<(
        i64,
        i64,
        Option<i64>,
        i64,
        String,
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, menu_id, parent_id, position, label, target_kind,
                target_entry_id, target_term_id, external_url
           FROM site_menu_items
          WHERE menu_id = ?
          ORDER BY COALESCE(parent_id, 0) ASC, position ASC, id ASC",
    )
    .bind(menu_id)
    .fetch_all(&mut *conn)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| MenuItemRecord {
            id: r.0,
            menu_id: r.1,
            parent_id: r.2,
            position: r.3,
            label: r.4,
            target_kind: r.5,
            target_entry_id: r.6,
            target_term_id: r.7,
            external_url: r.8,
        })
        .collect();
    Ok(Some(MenuWithItems { menu, items }))
}

pub async fn get_menu_by_key_in_tx(
    conn: &mut SqliteConnection,
    menu_key: &str,
) -> ApiResult<Option<MenuWithItems>> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM site_menus WHERE menu_key = ?")
        .bind(menu_key)
        .fetch_optional(&mut *conn)
        .await?;
    match id {
        Some(id) => get_menu_in_tx(conn, id).await,
        None => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey {
    Existing(i64),
    Client(String),
    Index(usize),
}

pub async fn replace_menu_items_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
    expected_version: i64,
    items: &[MenuItemInput],
) -> ApiResult<MenuWithItems> {
    if items.len() > MAX_MENU_ITEMS {
        return Err(ApiError::bad_request("Menu vượt quá giới hạn 200 items."));
    }
    let menu = get_menu_record_in_tx(conn, menu_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Không tìm thấy menu."))?;
    if menu.version != expected_version {
        return Err(ApiError::precondition_failed(
            "Menu đã được cập nhật ở phiên bản khác.",
        ));
    }

    // Existing item IDs belonging strictly to this menu.
    let existing_ids: BTreeSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT id FROM site_menu_items WHERE menu_id = ?")
            .bind(menu_id)
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();

    // Assign node keys and validate targets & local parent references.
    let mut keys: Vec<NodeKey> = Vec::with_capacity(items.len());
    let mut key_to_idx: BTreeMap<NodeKey, usize> = BTreeMap::new();
    for (idx, item) in items.iter().enumerate() {
        validate_label(&item.label)?;
        if item.position < 0 {
            return Err(ApiError::bad_request("Vị trí menu item không hợp lệ."));
        }
        match item.target_kind {
            MenuTargetKind::Entry => {
                if item.target_entry_id.is_none_or(|id| id <= 0) {
                    return Err(ApiError::bad_request("Menu item entry thiếu target_entry_id."));
                }
            }
            MenuTargetKind::Term => {
                if item.target_term_id.is_none_or(|id| id <= 0) {
                    return Err(ApiError::bad_request("Menu item term thiếu target_term_id."));
                }
            }
            MenuTargetKind::External => {
                let raw = item
                    .external_url
                    .as_deref()
                    .ok_or_else(|| ApiError::bad_request("Menu item external thiếu external_url."))?;
                validate_external_url(raw)?;
            }
        }
        let key = if let Some(id) = item.id {
            if !existing_ids.contains(&id) {
                return Err(ApiError::bad_request(
                    "Menu item id không thuộc menu hiện tại.",
                ));
            }
            NodeKey::Existing(id)
        } else if let Some(ref ck) = item.client_key {
            NodeKey::Client(ck.clone())
        } else {
            NodeKey::Index(idx)
        };
        if key_to_idx.insert(key.clone(), idx).is_some() {
            return Err(ApiError::bad_request("Trùng key menu item trong batch."));
        }
        keys.push(key);
    }

    // Resolve parent index for each item (must belong to same menu batch), check sibling position uniqueness & cycles/depth.
    let mut parent_idx: Vec<Option<usize>> = vec![None; items.len()];
    let mut sibling_positions: BTreeSet<(Option<usize>, i64)> = BTreeSet::new();
    for (idx, item) in items.iter().enumerate() {
        let p_idx = if let Some(pid) = item.parent_id {
            let found = key_to_idx.get(&NodeKey::Existing(pid)).copied().ok_or_else(|| {
                ApiError::bad_request("parent_id không thuộc cùng menu hoặc không tồn tại.")
            })?;
            Some(found)
        } else if let Some(ref pck) = item.parent_client_key {
            let found = key_to_idx.get(&NodeKey::Client(pck.clone())).copied().ok_or_else(|| {
                ApiError::bad_request("parent_client_key không tồn tại trong menu.")
            })?;
            Some(found)
        } else {
            None
        };
        if !sibling_positions.insert((p_idx, item.position)) {
            return Err(ApiError::bad_request(
                "Trùng position trong cùng cấp menu.",
            ));
        }
        parent_idx[idx] = p_idx;
    }

    // Check cycles and max depth <= 8 (root = depth 1).
    for start in 0..items.len() {
        let mut depth = 1usize;
        let mut cur = parent_idx[start];
        let mut seen = BTreeSet::new();
        seen.insert(start);
        while let Some(p) = cur {
            if !seen.insert(p) {
                return Err(ApiError::bad_request("Phát hiện vòng lặp (cycle) trong menu."));
            }
            depth += 1;
            if depth > MAX_MENU_DEPTH {
                return Err(ApiError::bad_request(
                    "Độ sâu menu vượt quá giới hạn 8 cấp.",
                ));
            }
            cur = parent_idx[p];
        }
    }

    // Topological order so parents are inserted/updated before children.
    let mut order = Vec::with_capacity(items.len());
    let mut placed = vec![false; items.len()];
    while order.len() < items.len() {
        let mut progressed = false;
        for idx in 0..items.len() {
            if !placed[idx] && parent_idx[idx].is_none_or(|p| placed[p]) {
                placed[idx] = true;
                order.push(idx);
                progressed = true;
            }
        }
        if !progressed {
            return Err(ApiError::bad_request("Không thể sắp xếp cây menu."));
        }
    }

    // Clear and reinsert preserving stable IDs for retained items.
    sqlx::query("DELETE FROM site_menu_items WHERE menu_id = ?")
        .bind(menu_id)
        .execute(&mut *conn)
        .await?;

    let mut assigned_ids: Vec<Option<i64>> = vec![None; items.len()];
    for idx in order {
        let item = &items[idx];
        let p_id = parent_idx[idx].and_then(|p| assigned_ids[p]);
        let label = validate_label(&item.label)?;
        let ext_url = match item.target_kind {
            MenuTargetKind::External => {
                Some(validate_external_url(item.external_url.as_deref().unwrap_or(""))?)
            }
            _ => None,
        };
        let entry_id = if item.target_kind == MenuTargetKind::Entry {
            item.target_entry_id
        } else {
            None
        };
        let term_id = if item.target_kind == MenuTargetKind::Term {
            item.target_term_id
        } else {
            None
        };
        let row_id: i64 = if let Some(stable_id) = item.id {
            sqlx::query_scalar(
                "INSERT INTO site_menu_items (
                    id, menu_id, parent_id, position, label, target_kind,
                    target_entry_id, target_term_id, external_url
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 RETURNING id",
            )
            .bind(stable_id)
            .bind(menu_id)
            .bind(p_id)
            .bind(item.position)
            .bind(&label)
            .bind(item.target_kind.as_str())
            .bind(entry_id)
            .bind(term_id)
            .bind(ext_url.as_deref())
            .fetch_one(&mut *conn)
            .await?
        } else {
            sqlx::query_scalar(
                "INSERT INTO site_menu_items (
                    menu_id, parent_id, position, label, target_kind,
                    target_entry_id, target_term_id, external_url
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 RETURNING id",
            )
            .bind(menu_id)
            .bind(p_id)
            .bind(item.position)
            .bind(&label)
            .bind(item.target_kind.as_str())
            .bind(entry_id)
            .bind(term_id)
            .bind(ext_url.as_deref())
            .fetch_one(&mut *conn)
            .await?
        };
        assigned_ids[idx] = Some(row_id);
    }

    let res = sqlx::query(
        "UPDATE site_menus
            SET version = version + 1,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE id = ? AND version = ?",
    )
    .bind(menu_id)
    .bind(expected_version)
    .execute(&mut *conn)
    .await?;
    if res.rows_affected() == 0 {
        return Err(ApiError::precondition_failed(
            "Menu đã thay đổi đồng thời.",
        ));
    }

    get_menu_in_tx(conn, menu_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được menu sau khi cập nhật."))
}

pub async fn assign_menu_slot_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    slot_key: &str,
    menu_id: Option<i64>,
) -> ApiResult<()> {
    validate_menu_key(slot_key)?;
    if let Some(mid) = menu_id {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM site_menus WHERE id = ?")
            .bind(mid)
            .fetch_optional(&mut *conn)
            .await?;
        if exists.is_none() {
            return Err(ApiError::not_found("Không tìm thấy menu để gắn slot."));
        }
        sqlx::query(
            "INSERT INTO site_menu_slot_assignments (theme_id, slot_key, menu_id)
             VALUES (?, ?, ?)
             ON CONFLICT(theme_id, slot_key) DO UPDATE SET menu_id = excluded.menu_id",
        )
        .bind(theme_id)
        .bind(slot_key)
        .bind(mid)
        .execute(&mut *conn)
        .await?;
    } else {
        sqlx::query("DELETE FROM site_menu_slot_assignments WHERE theme_id = ? AND slot_key = ?")
            .bind(theme_id)
            .bind(slot_key)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn list_slot_assignments_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
) -> ApiResult<BTreeMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT slot_key, menu_id FROM site_menu_slot_assignments WHERE theme_id = ? ORDER BY slot_key ASC",
    )
    .bind(theme_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().collect())
}

async fn resolve_entry_url_in_tx(conn: &mut SqliteConnection, entry_id: i64) -> ApiResult<Option<String>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT e.content_type_key, e.slug
           FROM content_entries e
           JOIN content_types t ON t.key = e.content_type_key
          WHERE e.id = ?
            AND e.publication_status = 'published'
            AND t.provider_status = 'active'",
    )
    .bind(entry_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.and_then(|(type_key, slug)| CanonicalUrlBuilder::entry_from_id(entry_id, &type_key, &slug)))
}

async fn resolve_term_url_in_tx(conn: &mut SqliteConnection, term_id: i64) -> ApiResult<Option<String>> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT kind, slug FROM taxonomy_terms WHERE id = ?")
            .bind(term_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.and_then(|(kind, slug)| {
        let kind = match kind.as_str() {
            "category" => TaxonomyKind::Category,
            "tag" => TaxonomyKind::Tag,
            _ => return None,
        };
        CanonicalUrlBuilder::term_from_id(term_id, kind, &slug)
    }))
}

pub async fn resolve_menu_view_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
) -> ApiResult<Option<MenuViewModel>> {
    let Some(with_items) = get_menu_in_tx(conn, menu_id).await? else {
        return Ok(None);
    };
    let mut diagnostics = Vec::new();
    let mut resolved_items: BTreeMap<i64, (Option<i64>, i64, String, String)> = BTreeMap::new();
    let mut children_map: BTreeMap<Option<i64>, Vec<i64>> = BTreeMap::new();

    for item in &with_items.items {
        let url_opt = match item.target_kind.as_str() {
            "entry" => match item.target_entry_id {
                Some(eid) => match resolve_entry_url_in_tx(conn, eid).await? {
                    Some(url) => Some(url),
                    None => {
                        diagnostics.push(MenuDiagnostic {
                            item_id: item.id,
                            code: "menu_target_entry_unavailable",
                            message: format!("Entry #{eid} chưa publish hoặc đã bị xóa."),
                        });
                        None
                    }
                },
                None => {
                    diagnostics.push(MenuDiagnostic {
                        item_id: item.id,
                        code: "menu_target_entry_missing",
                        message: "Thiếu target_entry_id.".to_string(),
                    });
                    None
                }
            },
            "term" => match item.target_term_id {
                Some(tid) => match resolve_term_url_in_tx(conn, tid).await? {
                    Some(url) => Some(url),
                    None => {
                        diagnostics.push(MenuDiagnostic {
                            item_id: item.id,
                            code: "menu_target_term_unavailable",
                            message: format!("Term #{tid} không còn tồn tại."),
                        });
                        None
                    }
                },
                None => {
                    diagnostics.push(MenuDiagnostic {
                        item_id: item.id,
                        code: "menu_target_term_missing",
                        message: "Thiếu target_term_id.".to_string(),
                    });
                    None
                }
            },
            "external" => match item.external_url.as_deref() {
                Some(raw) => match validate_external_url(raw) {
                    Ok(url) => Some(url),
                    Err(_) => {
                        diagnostics.push(MenuDiagnostic {
                            item_id: item.id,
                            code: "menu_target_external_invalid",
                            message: "External URL không hợp lệ.".to_string(),
                        });
                        None
                    }
                },
                None => None,
            },
            _ => None,
        };

        if let Some(url) = url_opt {
            resolved_items.insert(item.id, (item.parent_id, item.position, item.label.clone(), url));
            children_map.entry(item.parent_id).or_default().push(item.id);
        }
    }

    fn build_tree(
        parent: Option<i64>,
        resolved: &BTreeMap<i64, (Option<i64>, i64, String, String)>,
        children_map: &BTreeMap<Option<i64>, Vec<i64>>,
        depth: usize,
    ) -> Vec<MenuItemView> {
        if depth > MAX_MENU_DEPTH {
            return Vec::new();
        }
        let Some(ids) = children_map.get(&parent) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            if let Some((_, _, label, url)) = resolved.get(&id) {
                let children = build_tree(Some(id), resolved, children_map, depth + 1);
                out.push(MenuItemView {
                    id,
                    label: label.clone(),
                    url: url.clone(),
                    children,
                });
            }
        }
        out
    }

    let tree = build_tree(None, &resolved_items, &children_map, 1);
    Ok(Some(MenuViewModel {
        menu_id: with_items.menu.id,
        menu_key: with_items.menu.menu_key,
        title: with_items.menu.title,
        items: tree,
        diagnostics,
    }))
}

pub async fn resolve_active_slot_menus(
    db: &SqlitePool,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let mut tx = db.begin().await?;
    let active_theme_id: Option<String> = sqlx::query_scalar(
        "SELECT r.theme_id
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(theme_id) = active_theme_id else {
        tx.commit().await?;
        return Ok(BTreeMap::new());
    };
    let assignments = list_slot_assignments_in_tx(&mut tx, &theme_id).await?;
    let mut out = BTreeMap::new();
    for (slot_key, menu_id) in assignments {
        if let Some(view) = resolve_menu_view_in_tx(&mut tx, menu_id).await? {
            out.insert(slot_key, view);
        }
    }
    tx.commit().await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::init_db;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn item_ext(label: &str, pos: i64, url: &str) -> MenuItemInput {
        MenuItemInput {
            id: None,
            client_key: None,
            parent_id: None,
            parent_client_key: None,
            position: pos,
            label: label.to_string(),
            target_kind: MenuTargetKind::External,
            target_entry_id: None,
            target_term_id: None,
            external_url: Some(url.to_string()),
        }
    }

    #[tokio::test]
    async fn site_menus_bounds_cycles_urls_cas_and_cross_menu_parent() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("opendash-m5-s09-t01-unit-{nanos}"));
        let db = init_db(&dir.join("cms.db")).await.unwrap();

        let mut tx = db.begin().await.unwrap();
        let m1 = create_menu_in_tx(
            &mut tx,
            &CreateMenuInput {
                menu_key: "primary".into(),
                title: "Primary Menu".into(),
                origin_provenance: Some("theme-a:sample:primary".into()),
            },
        )
        .await
        .unwrap();
        let m2 = create_menu_in_tx(
            &mut tx,
            &CreateMenuInput {
                menu_key: "footer".into(),
                title: "Footer Menu".into(),
                origin_provenance: None,
            },
        )
        .await
        .unwrap();
        let dup_origin = create_menu_in_tx(
            &mut tx,
            &CreateMenuInput {
                menu_key: "primary-2".into(),
                title: "Dup Origin".into(),
                origin_provenance: Some("theme-a:sample:primary".into()),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(dup_origin.0, axum::http::StatusCode::CONFLICT);

        // > 200 items
        let too_many: Vec<MenuItemInput> = (0..201)
            .map(|i| item_ext(&format!("Item {i}"), i, "https://example.com"))
            .collect();
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_many).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // depth > 8
        let too_deep: Vec<MenuItemInput> = (1..=9)
            .map(|d| MenuItemInput {
                id: None,
                client_key: Some(format!("k{d}")),
                parent_id: None,
                parent_client_key: if d > 1 { Some(format!("k{}", d - 1)) } else { None },
                position: 0,
                label: format!("Depth {d}"),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com".into()),
            })
            .collect();
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_deep).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // cycle
        let cycle = vec![
            MenuItemInput {
                id: None,
                client_key: Some("a".into()),
                parent_id: None,
                parent_client_key: Some("b".into()),
                position: 0,
                label: "A".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com".into()),
            },
            MenuItemInput {
                id: None,
                client_key: Some("b".into()),
                parent_id: None,
                parent_client_key: Some("a".into()),
                position: 0,
                label: "B".into(),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com".into()),
            },
        ];
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &cycle).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // duplicate sibling position
        let dup_pos = vec![
            item_ext("One", 0, "https://example.com/1"),
            item_ext("Two", 0, "https://example.com/2"),
        ];
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &dup_pos).await.unwrap_err().0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // unsafe URLs
        for bad_url in [
            "javascript:alert(1)",
            "//evil.example.com/phish",
            "https://user:pass@example.com/secret",
            "https://example.com/\nheader",
        ] {
            assert_eq!(
                replace_menu_items_in_tx(&mut tx, m1.id, 1, &[item_ext("Bad", 0, bad_url)])
                    .await
                    .unwrap_err()
                    .0,
                axum::http::StatusCode::BAD_REQUEST
            );
        }

        // stale CAS
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 99, &[item_ext("Ok", 0, "https://example.com")])
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::PRECONDITION_FAILED
        );

        // valid hierarchy + cross-menu parent check
        let saved = replace_menu_items_in_tx(
            &mut tx,
            m1.id,
            1,
            &[
                MenuItemInput {
                    id: None,
                    client_key: Some("root".into()),
                    parent_id: None,
                    parent_client_key: None,
                    position: 0,
                    label: "Docs".into(),
                    target_kind: MenuTargetKind::External,
                    target_entry_id: None,
                    target_term_id: None,
                    external_url: Some("https://example.com/docs".into()),
                },
                MenuItemInput {
                    id: None,
                    client_key: Some("child".into()),
                    parent_id: None,
                    parent_client_key: Some("root".into()),
                    position: 0,
                    label: "Guide".into(),
                    target_kind: MenuTargetKind::External,
                    target_entry_id: None,
                    target_term_id: None,
                    external_url: Some("https://example.com/guide".into()),
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(saved.menu.version, 2);
        assert_eq!(saved.items.len(), 2);

        // preserve stable ID on update
        let root_id = saved.items[0].id;
        let child_id = saved.items[1].id;
        let saved2 = replace_menu_items_in_tx(
            &mut tx,
            m1.id,
            2,
            &[
                MenuItemInput {
                    id: Some(root_id),
                    client_key: None,
                    parent_id: None,
                    parent_client_key: None,
                    position: 0,
                    label: "Docs Updated".into(),
                    target_kind: MenuTargetKind::External,
                    target_entry_id: None,
                    target_term_id: None,
                    external_url: Some("https://example.com/docs".into()),
                },
                MenuItemInput {
                    id: Some(child_id),
                    client_key: None,
                    parent_id: Some(root_id),
                    parent_client_key: None,
                    position: 0,
                    label: "Guide Updated".into(),
                    target_kind: MenuTargetKind::External,
                    target_entry_id: None,
                    target_term_id: None,
                    external_url: Some("https://example.com/guide".into()),
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(saved2.items[0].id, root_id);
        assert_eq!(saved2.items[1].id, child_id);

        let cross_parent = MenuItemInput {
            id: None,
            client_key: None,
            parent_id: Some(root_id),
            parent_client_key: None,
            position: 0,
            label: "Cross".into(),
            target_kind: MenuTargetKind::External,
            target_entry_id: None,
            target_term_id: None,
            external_url: Some("https://example.com/cross".into()),
        };
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m2.id, 1, &[cross_parent])
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        tx.commit().await.unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
