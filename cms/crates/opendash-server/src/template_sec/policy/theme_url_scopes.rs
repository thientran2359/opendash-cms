use super::*;

pub(crate) fn resolve_field_node_and_tail<'p, 'a, 's>(
    root_id: &str,
    attrs: &'a [&'s str],
    scopes: &'p UrlScopeStack,
    policy: &'p ThemeUrlPolicy,
) -> Option<(&'p FieldTemplateNode, &'a [&'s str])> {
    let (mut node, mut rest): (&FieldTemplateNode, &[&'s str]) = match root_id {
        "options" => {
            let first = *attrs.first()?;
            (policy.options_tree.get(first)?, &attrs[1..])
        }
        "site_options" => {
            let first = *attrs.first()?;
            (policy.site_options_tree.get(first)?, &attrs[1..])
        }
        other => match scopes.lookup(other) {
            Some(UrlSourceKind::FieldNode(node)) => (node, attrs),
            Some(UrlSourceKind::TrustedEntry) if attrs.len() >= 2 && attrs[0] == "fields" => {
                (policy.entry_field_nodes.get(attrs[1])?, &attrs[2..])
            }
            _ => return None,
        },
    };
    while let FieldTemplateNode::Group(children) = node
        && !rest.is_empty()
    {
        let child = children.get(rest[0])?;
        node = child;
        rest = &rest[1..];
    }
    Some((node, rest))
}

pub(crate) fn is_richtext_field_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    let Some((root, attrs)) = extract_attr_chain(expr) else {
        return false;
    };
    matches!(
        resolve_field_node_and_tail(root, &attrs, scopes, policy),
        Some((FieldTemplateNode::Richtext, &[]))
    )
}

pub(crate) fn resolve_collection_source(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<UrlSourceKind> {
    match expr {
        Expr::Slice(s) => resolve_collection_source(&s.expr, scopes, policy),
        Expr::Filter(f) => {
            let inner = f.expr.as_ref()?;
            if matches!(f.name, "selectattr" | "rejectattr" | "sort" | "reverse") {
                resolve_collection_source(inner, scopes, policy)
            } else if f.name == "batch" {
                match resolve_collection_source(inner, scopes, policy) {
                    Some(UrlSourceKind::TrustedEntry) => Some(UrlSourceKind::TrustedEntryBatch),
                    _ => None,
                }
            } else {
                None
            }
        }
        Expr::Var(v) => {
            if matches!(v.id, "entries" | "related_entries" | "recent_posts") {
                Some(UrlSourceKind::TrustedEntry)
            } else if v.id == "breadcrumbs" {
                Some(UrlSourceKind::TrustedLink)
            } else if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntryBatch) {
                Some(UrlSourceKind::TrustedEntry)
            } else {
                None
            }
        }
        Expr::GetAttr(g) => {
            if let Some((root, attrs)) = extract_attr_chain(expr)
                && let Some((node, &[])) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
            {
                return match node {
                    FieldTemplateNode::Repeater(fields) => Some(UrlSourceKind::FieldNode(
                        FieldTemplateNode::Group(fields.clone()),
                    )),
                    FieldTemplateNode::Gallery => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::Media))
                    }
                    FieldTemplateNode::EntryRef { .. } => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::EntryRef {
                            multiple: false,
                        }))
                    }
                    FieldTemplateNode::TermRef { .. } => {
                        Some(UrlSourceKind::FieldNode(FieldTemplateNode::TermRef {
                            multiple: false,
                        }))
                    }
                    _ => None,
                };
            }
            match &g.expr {
                Expr::Var(v) if v.id == "pagination" && g.name == "pages" => {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v) if v.id == "locale" && g.name == "alternates" => {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::GetAttr(menu_slot)
                    if g.name == "items"
                        && matches!(&menu_slot.expr, Expr::Var(v) if v.id == "menus") =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::GetItem(menu_slot)
                    if g.name == "items"
                        && matches!(&menu_slot.expr, Expr::Var(v) if v.id == "menus")
                        && matches!(&menu_slot.subscript_expr, Expr::Const(c) if c.value.as_str().is_some()) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v)
                    if g.name == "children"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedLink) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v)
                    if g.name == "all_terms"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::GetAttr(terms_attr)
                    if terms_attr.name == "terms"
                        && matches!(
                            &terms_attr.expr,
                            Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
                        ) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                Expr::Var(v)
                    if g.name == "field_list"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
                {
                    Some(UrlSourceKind::TrustedField)
                }
                Expr::Var(v)
                    if g.name == "gallery"
                        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedField) =>
                {
                    Some(UrlSourceKind::FieldNode(FieldTemplateNode::Media))
                }
                _ => None,
            }
        }
        Expr::GetItem(g) => {
            let Expr::Const(c) = &g.subscript_expr else {
                return None;
            };
            let _tax = c.value.as_str()?;
            match &g.expr {
                Expr::GetAttr(terms_attr)
                    if terms_attr.name == "terms"
                        && matches!(
                            &terms_attr.expr,
                            Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
                        ) =>
                {
                    Some(UrlSourceKind::TrustedLink)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn is_href_only_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        if match (node, tail) {
            (FieldTemplateNode::Url, &[]) => true,
            (FieldTemplateNode::Link, &["url"]) => true,
            (FieldTemplateNode::EntryRef { multiple: false }, &["url"]) => true,
            (FieldTemplateNode::TermRef { multiple: false }, &["url"]) => true,
            _ => false,
        } {
            return true;
        }
    }
    match expr {
        Expr::Var(v) => matches!(v.id, "archive_url" | "previous_url" | "next_url"),
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "pagination" => {
                matches!(g.name, "previous_url" | "next_url")
            }
            Expr::Var(v) if v.id == "locale" => g.name == "home_url",
            Expr::Var(v) if v.id == "options" => {
                policy.allows_cite_href(&format!("options.{}", g.name))
            }
            Expr::Var(v)
                if g.name == "url"
                    && matches!(
                        scopes.lookup(v.id),
                        Some(UrlSourceKind::TrustedEntry | UrlSourceKind::TrustedLink)
                    ) =>
            {
                true
            }
            Expr::GetAttr(cat_attr)
                if g.name == "url"
                    && cat_attr.name == "primary_category"
                    && matches!(
                        &cat_attr.expr,
                        Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry)
                    ) =>
            {
                true
            }
            Expr::GetAttr(link_attr)
                if g.name == "url"
                    && link_attr.name == "link"
                    && matches!(
                        &link_attr.expr,
                        Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedField)
                    ) =>
            {
                true
            }
            _ => false,
        },
        _ => false,
    }
}

pub(crate) fn is_src_and_href_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        if match (node, tail) {
            (FieldTemplateNode::Media, &["url" | "thumbnail_url"]) => true,
            (FieldTemplateNode::EntryRef { multiple: false }, &["cover_url"]) => true,
            (
                FieldTemplateNode::TermRef { multiple: false },
                &["featured_image", "url" | "thumbnail_url"],
            ) => true,
            _ => false,
        } {
            return true;
        }
    }
    match expr {
        Expr::Var(v) => v.id == "theme_asset_url",
        Expr::GetAttr(g) => match &g.expr {
            Expr::Var(v) if v.id == "options" => {
                policy.allows_media_src(&format!("options.{}", g.name))
            }
            Expr::Var(v)
                if g.name == "cover_url"
                    && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedEntry) =>
            {
                true
            }
            Expr::GetAttr(media_attr)
                if matches!(g.name, "url" | "thumbnail_url")
                    && media_attr.name == "media"
                    && matches!(
                        &media_attr.expr,
                        Expr::Var(v) if scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedField)
                    ) =>
            {
                true
            }
            _ => false,
        },
        Expr::GetItem(g) => {
            matches!(&g.expr, Expr::Var(v) if v.id == "theme_assets")
                && matches!(
                    &g.subscript_expr,
                    Expr::Const(c) if c.value.as_str().is_some_and(|p| policy.allows_declared_asset(p))
                )
        }
        _ => false,
    }
}

pub(crate) fn is_valid_theme_url_expr(
    expr: &Expr<'_>,
    slot: UrlAttrSlot,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match slot {
        UrlAttrSlot::AnchorHref => {
            is_href_only_expr(expr, scopes, policy) || is_src_and_href_expr(expr, scopes, policy)
        }
        UrlAttrSlot::Cite => is_href_only_expr(expr, scopes, policy),
        UrlAttrSlot::SrcAndHref => is_src_and_href_expr(expr, scopes, policy),
        UrlAttrSlot::LinkCss => {
            matches!(&expr, Expr::GetItem(g)
                if matches!(&g.expr, Expr::Var(v) if v.id == "theme_assets")
                    && matches!(
                        &g.subscript_expr,
                        Expr::Const(c) if c.value.as_str().is_some_and(|p| {
                            policy.allows_declared_asset(p)
                                && is_safe_theme_relative_path(p)
                                && p.ends_with(".css")
                        })
                    )
            )
        }
    }
}

pub(crate) fn walk_theme_ast_with_url_scopes(
    stmt: &Stmt<'_>,
    is_part: bool,
    is_full_doctype_doc: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
) -> Result<ThemeTemplateUsageFlags, &'static str> {
    let mut usage = ThemeTemplateUsageFlags::default();
    let (final_states, _) = walk_theme_stmt_with_url_scopes(
        stmt,
        BTreeSet::from([ThemeHtmlState::with_trust(
            ThemeHtmlState::Body,
            allow_trusted_theme_markup,
            0,
        )]),
        UrlScopeStack::root(),
        is_part,
        is_full_doctype_doc,
        policy,
        allow_trusted_theme_markup,
        &mut usage,
    )?;
    if final_states
        .iter()
        .any(|state| state.inner() != &ThemeHtmlState::Body || state.trusted_depth() != 0)
    {
        return Err(
            "Template phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment/style/title).",
        );
    }
    Ok(usage)
}

pub(crate) fn walk_theme_stmts_with_url_scopes(
    stmts: &[Stmt<'_>],
    mut states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    is_part: bool,
    is_full_doctype_doc: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
    usage: &mut ThemeTemplateUsageFlags,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    for child in stmts {
        let (next_states, next_scopes) = walk_theme_stmt_with_url_scopes(
            child,
            states,
            scopes,
            is_part,
            is_full_doctype_doc,
            policy,
            allow_trusted_theme_markup,
            usage,
        )?;
        states = next_states;
        scopes = next_scopes;
        if states.len() > MAX_THEME_WALKER_STATES {
            return Err("Quá nhiều trạng thái HTML trong theme template.");
        }
    }
    Ok((states, scopes))
}
