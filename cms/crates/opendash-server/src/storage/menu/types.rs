//! SITE-01 site-owned menus, hierarchical items, slot assignments, and
//! canonical MenuViewModel rendering.

use super::*;

pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use serde::{Deserialize, Serialize};

pub(super) use sqlx::{SqliteConnection, SqlitePool};

pub(super) use url::Url;

pub(super) use crate::error::{ApiError, ApiResult};

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
pub struct MenuWithItems {
    pub menu: MenuRecord,
    pub items: Vec<MenuItemRecord>,
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
    let dup_key: Option<i64> = sqlx::query_scalar("SELECT id FROM site_menus WHERE menu_key = ?")
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

pub(in crate::storage) async fn get_menu_record_in_tx(
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

pub(in crate::storage) fn validate_menu_items_plan(
    existing_ids: &BTreeSet<i64>,
    items: &[MenuItemInput],
) -> ApiResult<ValidatedMenuPlan> {
    if items.len() > MAX_MENU_ITEMS {
        return Err(ApiError::bad_request("Menu vượt quá giới hạn 200 items."));
    }
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
                    return Err(ApiError::bad_request(
                        "Menu item entry thiếu target_entry_id.",
                    ));
                }
            }
            MenuTargetKind::Term => {
                if item.target_term_id.is_none_or(|id| id <= 0) {
                    return Err(ApiError::bad_request(
                        "Menu item term thiếu target_term_id.",
                    ));
                }
            }
            MenuTargetKind::External => {
                let raw = item.external_url.as_deref().ok_or_else(|| {
                    ApiError::bad_request("Menu item external thiếu external_url.")
                })?;
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

    let mut parent_idx: Vec<Option<usize>> = vec![None; items.len()];
    let mut sibling_positions: BTreeSet<(Option<usize>, i64)> = BTreeSet::new();
    for (idx, item) in items.iter().enumerate() {
        let p_idx = if let Some(pid) = item.parent_id {
            let found = key_to_idx
                .get(&NodeKey::Existing(pid))
                .copied()
                .ok_or_else(|| {
                    ApiError::bad_request("parent_id không thuộc cùng menu hoặc không tồn tại.")
                })?;
            Some(found)
        } else if let Some(ref pck) = item.parent_client_key {
            let found = key_to_idx
                .get(&NodeKey::Client(pck.clone()))
                .copied()
                .ok_or_else(|| {
                    ApiError::bad_request("parent_client_key không tồn tại trong menu.")
                })?;
            Some(found)
        } else {
            None
        };
        if !sibling_positions.insert((p_idx, item.position)) {
            return Err(ApiError::bad_request("Trùng position trong cùng cấp menu."));
        }
        parent_idx[idx] = p_idx;
    }

    for start in 0..items.len() {
        let mut depth = 1usize;
        let mut cur = parent_idx[start];
        let mut seen = BTreeSet::new();
        seen.insert(start);
        while let Some(p) = cur {
            if !seen.insert(p) {
                return Err(ApiError::bad_request(
                    "Phát hiện vòng lặp (cycle) trong menu.",
                ));
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

    Ok(ValidatedMenuPlan { parent_idx, order })
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
                    return Err(ApiError::bad_request(
                        "Menu item entry thiếu target_entry_id.",
                    ));
                }
            }
            MenuTargetKind::Term => {
                if item.target_term_id.is_none_or(|id| id <= 0) {
                    return Err(ApiError::bad_request(
                        "Menu item term thiếu target_term_id.",
                    ));
                }
            }
            MenuTargetKind::External => {
                let raw = item.external_url.as_deref().ok_or_else(|| {
                    ApiError::bad_request("Menu item external thiếu external_url.")
                })?;
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
            let found = key_to_idx
                .get(&NodeKey::Existing(pid))
                .copied()
                .ok_or_else(|| {
                    ApiError::bad_request("parent_id không thuộc cùng menu hoặc không tồn tại.")
                })?;
            Some(found)
        } else if let Some(ref pck) = item.parent_client_key {
            let found = key_to_idx
                .get(&NodeKey::Client(pck.clone()))
                .copied()
                .ok_or_else(|| {
                    ApiError::bad_request("parent_client_key không tồn tại trong menu.")
                })?;
            Some(found)
        } else {
            None
        };
        if !sibling_positions.insert((p_idx, item.position)) {
            return Err(ApiError::bad_request("Trùng position trong cùng cấp menu."));
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
                return Err(ApiError::bad_request(
                    "Phát hiện vòng lặp (cycle) trong menu.",
                ));
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
            MenuTargetKind::External => Some(validate_external_url(
                item.external_url.as_deref().unwrap_or(""),
            )?),
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
        return Err(ApiError::precondition_failed("Menu đã thay đổi đồng thời."));
    }

    get_menu_in_tx(conn, menu_id)
        .await?
        .ok_or_else(|| ApiError::internal("Không đọc lại được menu sau khi cập nhật."))
}

pub(in crate::storage) struct AbstractMenuItem {
    pub(in crate::storage) id: i64,
    pub(in crate::storage) parent_id: Option<i64>,
    pub(in crate::storage) label: String,
    pub(in crate::storage) target_kind: Option<MenuTargetKind>,
    pub(in crate::storage) target_entry_id: Option<i64>,
    pub(in crate::storage) target_term_id: Option<i64>,
    pub(in crate::storage) external_url: Option<String>,
}

pub(in crate::storage) async fn resolve_and_build_menu_view(
    conn: &mut SqliteConnection,
    menu: MenuRecord,
    preview_title: Option<String>,
    items: Vec<AbstractMenuItem>,
    current_path: Option<&str>,
    strict_preview_missing_id: bool,
) -> ApiResult<MenuViewModel> {
    let mut entry_ids = Vec::new();
    let mut term_ids = Vec::new();
    for item in &items {
        match item.target_kind {
            Some(MenuTargetKind::Entry) => {
                if let Some(eid) = item.target_entry_id {
                    entry_ids.push(eid);
                }
            }
            Some(MenuTargetKind::Term) => {
                if let Some(tid) = item.target_term_id {
                    term_ids.push(tid);
                }
            }
            _ => {}
        }
    }
    let entry_urls = if entry_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::seo::resolve_public_entry_paths_by_entry_ids_in_conn(conn, &entry_ids)
            .await?
    };
    let term_urls = if term_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::permalinks::batch_canonical_term_paths_in_tx(conn, &term_ids).await?
    };

    let normalized_current = current_path.map(normalize_menu_path);
    let mut diagnostics = Vec::new();
    let mut resolved_items: BTreeMap<i64, (String, String, bool)> = BTreeMap::new();
    let mut children_map: BTreeMap<Option<i64>, Vec<i64>> = BTreeMap::new();

    for item in items {
        let mut is_external = false;
        let url_opt = match item.target_kind {
            Some(MenuTargetKind::Entry) => match item.target_entry_id {
                Some(eid) => match entry_urls.get(&eid) {
                    Some(url) => Some(url.clone()),
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
                    if !strict_preview_missing_id {
                        diagnostics.push(MenuDiagnostic {
                            item_id: item.id,
                            code: "menu_target_entry_missing",
                            message: "Thiếu target_entry_id.".to_string(),
                        });
                    }
                    None
                }
            },
            Some(MenuTargetKind::Term) => match item.target_term_id {
                Some(tid) => match term_urls.get(&tid) {
                    Some(url) => Some(url.clone()),
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
                    if !strict_preview_missing_id {
                        diagnostics.push(MenuDiagnostic {
                            item_id: item.id,
                            code: "menu_target_term_missing",
                            message: "Thiếu target_term_id.".to_string(),
                        });
                    }
                    None
                }
            },
            Some(MenuTargetKind::External) => {
                is_external = true;
                match item.external_url.as_deref() {
                    Some(raw) => match validate_external_url(raw) {
                        Ok(url) => Some(url),
                        Err(_) => {
                            if !strict_preview_missing_id {
                                diagnostics.push(MenuDiagnostic {
                                    item_id: item.id,
                                    code: "menu_target_external_invalid",
                                    message: "External URL không hợp lệ.".to_string(),
                                });
                            }
                            None
                        }
                    },
                    None => None,
                }
            }
            None => None,
        };

        if let Some(url) = url_opt {
            resolved_items.insert(item.id, (item.label, url, is_external));
            children_map
                .entry(item.parent_id)
                .or_default()
                .push(item.id);
        }
    }

    fn build_tree(
        parent: Option<i64>,
        resolved: &BTreeMap<i64, (String, String, bool)>,
        children_map: &BTreeMap<Option<i64>, Vec<i64>>,
        normalized_current: Option<&str>,
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
            if let Some((label, url, is_external)) = resolved.get(&id) {
                let children = build_tree(
                    Some(id),
                    resolved,
                    children_map,
                    normalized_current,
                    depth + 1,
                );
                let is_active = !*is_external
                    && normalized_current.is_some_and(|cur| normalize_menu_path(url) == cur);
                let is_ancestor = children.iter().any(|c| c.is_active || c.is_ancestor);
                out.push(MenuItemView {
                    id,
                    label: label.clone(),
                    url: url.clone(),
                    is_active,
                    is_ancestor,
                    is_external: *is_external,
                    depth,
                    children,
                });
            }
        }
        out
    }

    let tree = build_tree(
        None,
        &resolved_items,
        &children_map,
        normalized_current.as_deref(),
        1,
    );
    Ok(MenuViewModel {
        menu_id: menu.id,
        menu_key: menu.menu_key,
        title: preview_title.unwrap_or(menu.title),
        items: tree,
        diagnostics,
    })
}

pub async fn resolve_menu_view_for_path_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
    current_path: Option<&str>,
) -> ApiResult<Option<MenuViewModel>> {
    let Some(with_items) = get_menu_in_tx(conn, menu_id).await? else {
        return Ok(None);
    };
    let items = with_items
        .items
        .into_iter()
        .map(|item| AbstractMenuItem {
            id: item.id,
            parent_id: item.parent_id,
            label: item.label,
            target_kind: MenuTargetKind::parse(&item.target_kind).ok(),
            target_entry_id: item.target_entry_id,
            target_term_id: item.target_term_id,
            external_url: item.external_url,
        })
        .collect();
    let view = resolve_and_build_menu_view(conn, with_items.menu, None, items, current_path, false)
        .await?;
    Ok(Some(view))
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuListPage {
    pub items: Vec<MenuRecord>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_menus(db: &SqlitePool, limit: i64, offset: i64) -> ApiResult<MenuListPage> {
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit phải trong khoảng 1..=100."));
    }
    if offset < 0 {
        return Err(ApiError::bad_request("offset không hợp lệ."));
    }
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_menus")
        .fetch_one(db)
        .await?;
    let rows: Vec<(i64, String, String, i64, Option<String>, String)> = sqlx::query_as(
        "SELECT id, menu_key, title, version, origin_provenance, updated_at
           FROM site_menus
          ORDER BY id ASC
          LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| MenuRecord {
            id: r.0,
            menu_key: r.1,
            title: r.2,
            version: r.3,
            origin_provenance: r.4,
            updated_at: r.5,
        })
        .collect();
    Ok(MenuListPage {
        items,
        total,
        limit,
        offset,
    })
}

pub async fn create_menu_admin(
    db: &SqlitePool,
    menu_key: &str,
    title: &str,
) -> ApiResult<MenuWithItems> {
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let menu = create_menu_in_tx(
            &mut conn,
            &CreateMenuInput {
                menu_key: menu_key.to_owned(),
                title: title.to_owned(),
                origin_provenance: None,
            },
        )
        .await?;
        Ok::<_, ApiError>(MenuWithItems {
            menu,
            items: Vec::new(),
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

pub async fn update_menu_with_title(
    db: &SqlitePool,
    menu_id: i64,
    expected_version: i64,
    title: &str,
    items: &[MenuItemInput],
) -> ApiResult<MenuWithItems> {
    let clean_title = validate_label(title)?;
    let mut conn = db.acquire().await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
    let res = async {
        let _ = replace_menu_items_in_tx(&mut conn, menu_id, expected_version, items).await?;
        sqlx::query("UPDATE site_menus SET title = ? WHERE id = ?")
            .bind(&clean_title)
            .bind(menu_id)
            .execute(&mut *conn)
            .await?;
        get_menu_in_tx(&mut conn, menu_id)
            .await?
            .ok_or_else(|| ApiError::internal("Không đọc lại được menu sau khi cập nhật."))
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

/// Read-only preview of an unsaved menu tree using the canonical view projection.
pub async fn preview_menu(
    db: &SqlitePool,
    menu_id: i64,
    title_override: Option<&str>,
    items: &[MenuItemInput],
) -> ApiResult<Option<MenuViewModel>> {
    let mut tx = db.begin().await?;
    let Some(menu) = get_menu_record_in_tx(&mut tx, menu_id).await? else {
        tx.commit().await?;
        return Ok(None);
    };
    let preview_title = match title_override {
        Some(raw) => validate_label(raw)?,
        None => menu.title.clone(),
    };
    let existing_ids: BTreeSet<i64> =
        sqlx::query_scalar::<_, i64>("SELECT id FROM site_menu_items WHERE menu_id = ?")
            .bind(menu_id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let plan = validate_menu_items_plan(&existing_ids, items)?;

    let mut synthetic_ids: Vec<i64> = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        synthetic_ids.push(item.id.unwrap_or(-((idx as i64) + 1)));
    }

    let mut ordered_indices: Vec<usize> = (0..items.len()).collect();
    ordered_indices.sort_by_key(|&idx| {
        let p_id = plan.parent_idx[idx].map(|p| synthetic_ids[p]);
        (p_id.unwrap_or(0), items[idx].position, synthetic_ids[idx])
    });

    let mut abstract_items = Vec::with_capacity(items.len());
    for idx in ordered_indices {
        let item = &items[idx];
        abstract_items.push(AbstractMenuItem {
            id: synthetic_ids[idx],
            parent_id: plan.parent_idx[idx].map(|p| synthetic_ids[p]),
            label: validate_label(&item.label)?,
            target_kind: Some(item.target_kind),
            target_entry_id: item.target_entry_id,
            target_term_id: item.target_term_id,
            external_url: item.external_url.clone(),
        });
    }

    let view = resolve_and_build_menu_view(
        &mut tx,
        menu,
        Some(preview_title),
        abstract_items,
        None,
        true,
    )
    .await?;
    tx.commit().await?;
    Ok(Some(view))
}
