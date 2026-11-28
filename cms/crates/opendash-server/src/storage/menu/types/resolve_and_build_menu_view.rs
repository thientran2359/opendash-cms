use super::*;

pub(in crate::storage) async fn resolve_and_build_menu_view(
    conn: &mut SqliteConnection,
    menu: MenuRecord,
    preview_title: Option<String>,
    items: Vec<AbstractMenuItem>,
    current_path: Option<&str>,
    strict_preview_missing_id: bool,
) -> ApiResult<MenuViewModel> {
    resolve_and_build_menu_view_for_locale(
        conn,
        menu,
        preview_title,
        items,
        current_path,
        strict_preview_missing_id,
        None,
    )
    .await
}

pub(in crate::storage) async fn resolve_and_build_menu_view_for_locale(
    conn: &mut SqliteConnection,
    menu: MenuRecord,
    preview_title: Option<String>,
    items: Vec<AbstractMenuItem>,
    current_path: Option<&str>,
    strict_preview_missing_id: bool,
    locale_filter: Option<&str>,
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
        crate::storage::seo::resolve_public_entry_paths_by_entry_ids_for_locale_in_conn(
            conn,
            &entry_ids,
            locale_filter,
        )
        .await?
    };
    let term_urls = if term_ids.is_empty() {
        BTreeMap::new()
    } else {
        crate::storage::permalinks::batch_canonical_term_paths_for_locale_in_tx(
            conn,
            &term_ids,
            locale_filter,
        )
        .await?
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
    resolve_menu_view_for_locale_and_path_in_tx(conn, menu_id, current_path, None).await
}

pub async fn resolve_menu_view_for_locale_and_path_in_tx(
    conn: &mut SqliteConnection,
    menu_id: i64,
    current_path: Option<&str>,
    locale_filter: Option<&str>,
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
    let view = resolve_and_build_menu_view_for_locale(
        conn,
        with_items.menu,
        None,
        items,
        current_path,
        false,
        locale_filter,
    )
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
