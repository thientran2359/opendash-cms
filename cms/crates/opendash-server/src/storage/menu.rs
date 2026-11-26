//! SITE-01 site-owned menus, hierarchical items, slot assignments, and
//! canonical MenuViewModel rendering.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sqlx::{SqliteConnection, SqlitePool};
use url::Url;

use crate::error::{ApiError, ApiResult};

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
    pub is_active: bool,
    pub is_ancestor: bool,
    pub is_external: bool,
    pub depth: usize,
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
        return Err(ApiError::bad_request(
            "URL menu chỉ chấp nhận http hoặc https.",
        ));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ApiError::bad_request(
            "URL menu không được chứa credentials.",
        ));
    }
    if parsed.host_str().is_none() {
        return Err(ApiError::bad_request("URL menu thiếu host."));
    }
    Ok(trimmed.to_string())
}

fn validate_label(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 160
        || trimmed.chars().any(|c| c.is_control())
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NodeKey {
    Existing(i64),
    Client(String),
    Index(usize),
}

struct ValidatedMenuPlan {
    parent_idx: Vec<Option<usize>>,
    order: Vec<usize>,
}

fn validate_menu_items_plan(
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

struct AbstractMenuItem {
    id: i64,
    parent_id: Option<i64>,
    label: String,
    target_kind: Option<MenuTargetKind>,
    target_entry_id: Option<i64>,
    target_term_id: Option<i64>,
    external_url: Option<String>,
}

async fn resolve_and_build_menu_view(
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

pub async fn resolve_active_slot_menus_for_path(
    db: &SqlitePool,
    current_path: Option<&str>,
    locale: Option<&str>,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let mut tx = db.begin().await?;
    let out = resolve_active_slot_menus_for_path_in_conn(&mut tx, current_path, locale).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn resolve_active_slot_menus_for_path_in_conn(
    conn: &mut SqliteConnection,
    current_path: Option<&str>,
    locale: Option<&str>,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let active_theme_id: Option<String> = sqlx::query_scalar(
        "SELECT r.theme_id
           FROM theme_active_selection a
           JOIN theme_revisions r ON r.revision_id = a.revision_id
          WHERE a.id = 1",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(theme_id) = active_theme_id else {
        return Ok(BTreeMap::new());
    };
    let resolved_locale = match locale {
        Some(loc) => loc.to_owned(),
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut *conn).await?,
    };
    resolve_slot_menus_for_locale_and_path_in_tx(conn, &theme_id, &resolved_locale, current_path)
        .await
}

pub(super) async fn resolve_slot_menus_for_locale_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    theme_id: &str,
    locale: &str,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    resolve_slot_menus_for_locale_and_path_in_tx(&mut **tx, theme_id, locale, None).await
}

pub(super) async fn resolve_slot_menus_for_locale_and_path_in_tx(
    conn: &mut SqliteConnection,
    theme_id: &str,
    locale: &str,
    current_path: Option<&str>,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let assignments = list_slot_assignments_for_locale_in_tx(conn, theme_id, locale).await?;
    let mut out = BTreeMap::new();
    for (slot_key, menu_id) in assignments {
        if let Some(view) = resolve_menu_view_for_path_in_tx(conn, menu_id, current_path).await? {
            out.insert(slot_key, view);
        }
    }
    Ok(out)
}

pub async fn resolve_all_menus_for_locale_and_path_in_conn(
    conn: &mut SqliteConnection,
    requested_locale: Option<&str>,
    current_path: Option<&str>,
) -> ApiResult<BTreeMap<String, serde_json::Value>> {
    let slot_menus =
        resolve_active_slot_menus_for_path_in_conn(&mut *conn, current_path, requested_locale)
            .await?;
    let mut out = BTreeMap::new();
    let mut by_menu_id: BTreeMap<i64, serde_json::Value> = BTreeMap::new();
    for (slot_key, view) in slot_menus {
        let menu_id = view.menu_id;
        let json_val = serde_json::to_value(&view)?;
        by_menu_id
            .entry(menu_id)
            .or_insert_with(|| json_val.clone());
        out.insert(slot_key, json_val);
    }

    let menu_rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, menu_key FROM site_menus ORDER BY id ASC")
            .fetch_all(&mut *conn)
            .await?;
    for (menu_id, menu_key) in menu_rows {
        let val = if let Some(cached) = by_menu_id.get(&menu_id) {
            cached.clone()
        } else if let Some(view) =
            resolve_menu_view_for_path_in_tx(&mut *conn, menu_id, current_path).await?
        {
            let json_val = serde_json::to_value(&view)?;
            by_menu_id.insert(menu_id, json_val.clone());
            json_val
        } else {
            continue;
        };
        out.entry(menu_key).or_insert(val);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuListPage {
    pub items: Vec<MenuRecord>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MenuSlotsSnapshot {
    pub theme_id: String,
    pub version: i64,
    pub is_active: bool,
    pub available_slots: Vec<String>,
    pub assignments: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
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

pub async fn get_menu(db: &SqlitePool, menu_id: i64) -> ApiResult<Option<MenuWithItems>> {
    let mut tx = db.begin().await?;
    let res = get_menu_in_tx(&mut tx, menu_id).await?;
    tx.commit().await?;
    Ok(res)
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

pub fn extract_declared_menu_slots(manifest_json: &str) -> Vec<String> {
    let Ok(manifest) = serde_json::from_str::<opendash_core::theme::ThemeV3Manifest>(manifest_json)
    else {
        return Vec::new();
    };
    if manifest.validate().is_err() {
        return Vec::new();
    }
    let Some(menus_val) = manifest.menus else {
        return Vec::new();
    };
    let mut slots = BTreeSet::new();
    let collect_arr = |arr: &[serde_json::Value], out: &mut BTreeSet<String>| {
        for item in arr {
            if let Some(s) = item.as_str() {
                if validate_menu_key(s).is_ok() {
                    out.insert(s.to_owned());
                }
            } else if let Some(obj) = item.as_object() {
                if let Some(s) = obj
                    .get("key")
                    .or_else(|| obj.get("slot_key"))
                    .and_then(|v| v.as_str())
                {
                    if validate_menu_key(s).is_ok() {
                        out.insert(s.to_owned());
                    }
                }
            }
        }
    };
    if let Some(arr) = menus_val.as_array() {
        collect_arr(arr, &mut slots);
    } else if let Some(obj) = menus_val.as_object() {
        if let Some(arr) = obj.get("slots").and_then(|v| v.as_array()) {
            collect_arr(arr, &mut slots);
        } else {
            for k in obj.keys() {
                if validate_menu_key(k).is_ok() {
                    slots.insert(k.clone());
                }
            }
        }
    }
    slots.into_iter().collect()
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
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
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
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_many)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // depth > 8
        let too_deep: Vec<MenuItemInput> = (1..=9)
            .map(|d| MenuItemInput {
                id: None,
                client_key: Some(format!("k{d}")),
                parent_id: None,
                parent_client_key: if d > 1 {
                    Some(format!("k{}", d - 1))
                } else {
                    None
                },
                position: 0,
                label: format!("Depth {d}"),
                target_kind: MenuTargetKind::External,
                target_entry_id: None,
                target_term_id: None,
                external_url: Some("https://example.com".into()),
            })
            .collect();
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &too_deep)
                .await
                .unwrap_err()
                .0,
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
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &cycle)
                .await
                .unwrap_err()
                .0,
            axum::http::StatusCode::BAD_REQUEST
        );

        // duplicate sibling position
        let dup_pos = vec![
            item_ext("One", 0, "https://example.com/1"),
            item_ext("Two", 0, "https://example.com/2"),
        ];
        assert_eq!(
            replace_menu_items_in_tx(&mut tx, m1.id, 1, &dup_pos)
                .await
                .unwrap_err()
                .0,
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
            replace_menu_items_in_tx(
                &mut tx,
                m1.id,
                99,
                &[item_ext("Ok", 0, "https://example.com")]
            )
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

    #[tokio::test]
    async fn menu_view_computes_normalize_path_active_bottom_up_ancestor_external_and_depth() {
        assert_eq!(normalize_menu_path("/"), "/");
        assert_eq!(normalize_menu_path("///"), "/");
        assert_eq!(normalize_menu_path("/posts/?page=2#top"), "/posts");
        assert_eq!(normalize_menu_path("/docs/guide/"), "/docs/guide");

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("opendash-menu-s02-unit-{nanos}"));
        let db = init_db(&dir.join("cms.db")).await.unwrap();

        let mut tx = db.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO content_entries (content_type_key, locale, translation_group_id, slug, title, values_json, publication_status, version)
             VALUES ('page', 'vi', 'tg_m1', 'parent-pg', 'Parent Page', '{}', 'draft', 1),
                    ('page', 'vi', 'tg_m2', 'child-pg', 'Child Page', '{}', 'draft', 1)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE content_entries SET publication_status = 'published', published_at = datetime('now')
              WHERE content_type_key = 'page' AND slug IN ('parent-pg', 'child-pg') AND locale = 'vi'",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let p_id: i64 = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE slug = 'parent-pg' AND locale = 'vi'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let c_id: i64 = sqlx::query_scalar(
            "SELECT id FROM content_entries WHERE slug = 'child-pg' AND locale = 'vi'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        crate::storage::permalinks::sync_claims_in_tx(&mut tx)
            .await
            .unwrap();

        let m = create_menu_in_tx(
            &mut tx,
            &CreateMenuInput {
                menu_key: "main".into(),
                title: "Main".into(),
                origin_provenance: None,
            },
        )
        .await
        .unwrap();
        replace_menu_items_in_tx(
            &mut tx,
            m.id,
            1,
            &[
                MenuItemInput {
                    id: None,
                    client_key: Some("root_page".into()),
                    parent_id: None,
                    parent_client_key: None,
                    position: 0,
                    label: "Parent".into(),
                    target_kind: MenuTargetKind::Entry,
                    target_entry_id: Some(p_id),
                    target_term_id: None,
                    external_url: None,
                },
                MenuItemInput {
                    id: None,
                    client_key: Some("child_page".into()),
                    parent_id: None,
                    parent_client_key: Some("root_page".into()),
                    position: 0,
                    label: "Child".into(),
                    target_kind: MenuTargetKind::Entry,
                    target_entry_id: Some(c_id),
                    target_term_id: None,
                    external_url: None,
                },
                item_ext("Ext", 1, "https://example.com/out"),
            ],
        )
        .await
        .unwrap();

        let vm = resolve_menu_view_for_path_in_tx(&mut tx, m.id, Some("/child-pg/?utm=1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(vm.items.len(), 2);
        let root_item = &vm.items[0];
        assert_eq!(root_item.depth, 1);
        assert!(!root_item.is_external);
        assert!(!root_item.is_active);
        assert!(root_item.is_ancestor);
        assert_eq!(root_item.children.len(), 1);
        assert_eq!(root_item.children[0].depth, 2);
        assert!(root_item.children[0].is_active);
        assert!(!root_item.children[0].is_ancestor);

        let ext_item = &vm.items[1];
        assert!(ext_item.is_external);
        assert!(!ext_item.is_active);
        assert!(!ext_item.is_ancestor);

        let preview_vm = resolve_menu_view_for_path_in_tx(&mut tx, m.id, None)
            .await
            .unwrap()
            .unwrap();
        assert!(!preview_vm.items[0].is_ancestor);
        assert!(!preview_vm.items[0].children[0].is_active);

        tx.commit().await.unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
