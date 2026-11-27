use super::*;

pub async fn list_studio_targets(
    db: &SqlitePool,
    themes_root: &Path,
) -> ApiResult<StudioTargetsResponse> {
    use TemplateCandidateKind as K;
    let active = theme::load_active_theme_v3(db, themes_root).await?;
    let overrides_list = list_overrides(db).await?;
    let overrides_map: BTreeMap<String, &TemplateOverrideRecord> = overrides_list
        .iter()
        .map(|row| (row.canonical_target_key.clone(), row))
        .collect();
    let content_types = content::list_content_types_with_meta(db).await?;

    let mut items = Vec::new();
    let mut seen = BTreeSet::new();

    for (part_key, label) in [("header", "Header"), ("footer", "Footer")] {
        let mut target = candidate(K::Part);
        target.part_key = Some(part_key.to_owned());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            target,
            label.to_owned(),
            "parts",
        )?;
    }
    if let Some(ref theme) = active {
        for part_key in theme.manifest.parts.keys() {
            if part_key == "header" || part_key == "footer" {
                continue;
            }
            let mut target = candidate(K::Part);
            target.part_key = Some(part_key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Part: {part_key}"),
                "parts",
            )?;
        }
    }

    for (kind, label) in [
        (K::Home, "Trang chủ (Home)"),
        (K::Page, "Trang mặc định (Page)"),
        (K::Single, "Chi tiết bài viết (Single)"),
        (K::Archive, "Danh sách lưu trữ (Archive)"),
        (K::Search, "Trang tìm kiếm (Search)"),
        (K::Index, "Index dự phòng (Index)"),
    ] {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            candidate(kind),
            label.to_owned(),
            "core",
        )?;
    }

    for ct in content_types {
        if ct.base.status != ProviderStatus::Active
            || ct.base.declaration_state != DeclarationState::Declared
        {
            continue;
        }
        let d = &ct.base.descriptor;
        if d.key == "pattern" || d.key == "section" || d.key == "page" || !d.settings.public {
            continue;
        }
        let mut single_target = candidate(K::Single);
        single_target.content_type = Some(d.key.clone());
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            single_target,
            format!("Chi tiết: {}", d.label),
            "content_types",
        )?;
        if d.settings.has_archive {
            let mut archive_target = candidate(K::Archive);
            archive_target.content_type = Some(d.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                archive_target,
                format!("Lưu trữ: {}", d.label),
                "content_types",
            )?;
        }
    }

    if let Some(ref theme) = active {
        for layout in &theme.manifest.page_layouts {
            let mut target = candidate(K::PageLayout);
            target.layout_key = Some(layout.key.clone());
            push_studio_target_item(
                &mut items,
                &mut seen,
                &overrides_map,
                active.as_ref(),
                target,
                format!("Bố cục trang: {}", layout.label),
                "page_layouts",
            )?;
        }
        for tpl in &theme.manifest.templates {
            if tpl.kind == K::SingleCustom
                && let Some(ck) = &tpl.custom_key
            {
                let mut target = candidate(K::SingleCustom);
                target.custom_key = Some(ck.clone());
                let label = tpl
                    .label
                    .clone()
                    .unwrap_or_else(|| format!("Mẫu tùy chỉnh: {ck}"));
                push_studio_target_item(
                    &mut items,
                    &mut seen,
                    &overrides_map,
                    active.as_ref(),
                    target,
                    label,
                    "custom_templates",
                )?;
            }
        }
    }

    for row in &overrides_list {
        push_studio_target_item(
            &mut items,
            &mut seen,
            &overrides_map,
            active.as_ref(),
            row.target.clone(),
            row.canonical_target_key.clone(),
            "overrides",
        )?;
    }

    Ok(StudioTargetsResponse {
        active_theme_id: active.as_ref().map(|a| a.manifest.theme_id.clone()),
        active_theme_name: active.as_ref().map(|a| a.manifest.name.clone()),
        items,
    })
}
