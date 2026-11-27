use super::*;

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
