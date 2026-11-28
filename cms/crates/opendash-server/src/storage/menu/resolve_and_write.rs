use super::*;

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

pub(in crate::storage) fn validate_label(raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 160
        || trimmed.chars().any(|c| c.is_control())
    {
        return Err(ApiError::bad_request("Nhãn menu item không hợp lệ."));
    }
    Ok(trimmed.to_string())
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
pub(in crate::storage) enum NodeKey {
    Existing(i64),
    Client(String),
    Index(usize),
}

pub(in crate::storage) struct ValidatedMenuPlan {
    pub(in crate::storage) parent_idx: Vec<Option<usize>>,
    pub(in crate::storage) order: Vec<usize>,
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

pub(in crate::storage) async fn resolve_slot_menus_for_locale_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    theme_id: &str,
    locale: &str,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    resolve_slot_menus_for_locale_and_path_in_tx(&mut **tx, theme_id, locale, None).await
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

/// Resolves up to `MAX_MENU_LOADS_PER_REQUEST` (`<= 8` combined with `already_loaded`)
/// menus by key for the active locale, blocking excess loads before running SQL.
pub async fn resolve_menus_by_keys_for_path_in_tx(
    conn: &mut SqliteConnection,
    menu_keys: &[String],
    current_path: Option<&str>,
    locale: Option<&str>,
    already_loaded: usize,
) -> ApiResult<BTreeMap<String, MenuViewModel>> {
    let mut unique_keys = Vec::new();
    let mut seen = BTreeSet::new();
    for raw_key in menu_keys {
        let trimmed = raw_key.trim();
        if trimmed.is_empty() {
            continue;
        }
        validate_menu_key(trimmed)?;
        if seen.insert(trimmed.to_owned()) {
            unique_keys.push(trimmed.to_owned());
        }
    }
    if already_loaded.saturating_add(unique_keys.len())
        > opendash_core::builder::MAX_MENU_LOADS_PER_REQUEST
    {
        return Err(ApiError::bad_request(
            "Số lượng menu tải trên mỗi yêu cầu vượt quá giới hạn tối đa 8.",
        ));
    }
    if unique_keys.is_empty() {
        return Ok(BTreeMap::new());
    }
    let resolved_locale = match locale {
        Some(loc) => loc.to_owned(),
        None => crate::storage::settings::get_default_locale_code_in_conn(&mut *conn).await?,
    };
    let keys_json = serde_json::to_string(&unique_keys)?;
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, menu_key
           FROM site_menus
          WHERE menu_key IN (SELECT value FROM json_each(?))
          ORDER BY id ASC",
    )
    .bind(keys_json)
    .fetch_all(&mut *conn)
    .await?;

    let mut out = BTreeMap::new();
    for (menu_id, menu_key) in rows {
        if let Some(view) = resolve_menu_view_for_locale_and_path_in_tx(
            conn,
            menu_id,
            current_path,
            Some(&resolved_locale),
        )
        .await?
        {
            out.insert(menu_key, view);
        }
    }
    Ok(out)
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

pub async fn get_menu(db: &SqlitePool, menu_id: i64) -> ApiResult<Option<MenuWithItems>> {
    let mut tx = db.begin().await?;
    let res = get_menu_in_tx(&mut tx, menu_id).await?;
    tx.commit().await?;
    Ok(res)
}

pub fn extract_declared_menu_slots(manifest_json: &str) -> Vec<String> {
    let Ok(manifest) = serde_json::from_str::<opendash_core::theme::ThemeManifest>(manifest_json)
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
