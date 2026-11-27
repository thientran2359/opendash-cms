use super::*;

pub(crate) struct BlockSchemaTemplateContext {
    pub(crate) policy: ThemeUrlPolicy,
    pub(crate) root_scopes: UrlScopeStack,
}

impl BlockSchemaTemplateContext {
    pub(crate) fn from_schema(schema: &opendash_core::block_schema::BlockSchemaV1) -> Self {
        let policy = ThemeUrlPolicy::from_option_specs(&[], BTreeSet::new());
        let mut root = BTreeMap::new();
        for field in &schema.fields {
            if let Some(node) = field_template_node_from_spec(field) {
                root.insert(field.key.clone(), Some(UrlSourceKind::FieldNode(node)));
            }
        }
        Self {
            policy,
            root_scopes: UrlScopeStack::from_root_map(root),
        }
    }
}

pub(crate) fn is_protected_block_binding(id: &str, scopes: &UrlScopeStack) -> bool {
    if is_forbidden_block_shadow_name(id) {
        return true;
    }
    match scopes.lookup(id) {
        None => false,
        Some(UrlSourceKind::FieldNode(FieldTemplateNode::Scalar)) => false,
        Some(_) => true,
    }
}

pub(crate) fn classify_node_and_tail(
    node: &FieldTemplateNode,
    tail: &[&str],
) -> Option<BlockSensitiveKind> {
    match (node, tail) {
        (FieldTemplateNode::Scalar, &[]) => None,
        (FieldTemplateNode::Scalar, &["classes" | "css" | "top" | "right" | "bottom" | "left"]) => {
            None
        }
        (FieldTemplateNode::Scalar, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Icon, &["name"]) => None,
        (FieldTemplateNode::Icon, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Richtext, &[]) => Some(BlockSensitiveKind::Richtext),
        (FieldTemplateNode::Richtext, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Url, &[]) => Some(BlockSensitiveKind::HrefUrl),
        (FieldTemplateNode::Url, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Link, &["url"]) => Some(BlockSensitiveKind::HrefUrl),
        (FieldTemplateNode::Link, &["label" | "target" | "rel" | "new_tab" | "nofollow"]) => None,
        (FieldTemplateNode::Link, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Media, &["url" | "thumbnail_url"]) => {
            Some(BlockSensitiveKind::MediaUrl)
        }
        (FieldTemplateNode::Media, &["id" | "alt" | "width" | "height" | "kind" | "version"]) => {
            None
        }
        (FieldTemplateNode::Media, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::Gallery, _)
        | (FieldTemplateNode::Group(_), _)
        | (FieldTemplateNode::Repeater(_), _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::EntryRef { multiple: false }, &["url"]) => {
            Some(BlockSensitiveKind::HrefUrl)
        }
        (FieldTemplateNode::EntryRef { multiple: false }, &["cover_url"]) => {
            Some(BlockSensitiveKind::MediaUrl)
        }
        (
            FieldTemplateNode::EntryRef { multiple: false },
            &["id" | "type_key" | "title" | "slug" | "excerpt"],
        ) => None,
        (FieldTemplateNode::EntryRef { .. }, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
        (FieldTemplateNode::TermRef { multiple: false }, &["url"]) => {
            Some(BlockSensitiveKind::HrefUrl)
        }
        (
            FieldTemplateNode::TermRef { multiple: false },
            &["featured_image", "url" | "thumbnail_url"],
        ) => Some(BlockSensitiveKind::MediaUrl),
        (
            FieldTemplateNode::TermRef { multiple: false },
            &[
                "featured_image",
                "id" | "alt" | "width" | "height" | "kind" | "version",
            ],
        ) => None,
        (
            FieldTemplateNode::TermRef { multiple: false },
            &["id" | "taxonomy_key" | "name" | "slug" | "description"],
        ) => None,
        (FieldTemplateNode::TermRef { .. }, _) => Some(BlockSensitiveKind::ObjectOrNonScalar),
    }
}

pub(crate) fn classify_block_sensitive_expr(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<BlockSensitiveKind> {
    if let Some((_sub, attrs)) = resolve_menu_get_item_subscript(expr) {
        return match attrs.as_slice() {
            ["title" | "menu_key" | "menu_id"] => None,
            _ => Some(BlockSensitiveKind::ObjectOrNonScalar),
        };
    }
    if let Some(attrs) = resolve_menu_dot_chain(expr) {
        return match attrs.as_slice() {
            [_slot, "title" | "menu_key" | "menu_id"] => None,
            _ => Some(BlockSensitiveKind::ObjectOrNonScalar),
        };
    }
    if let Some((root, attrs)) = extract_attr_chain(expr) {
        if attrs.is_empty()
            && matches!(root, "options" | "site_options" | "menus" | "theme_options")
        {
            return Some(BlockSensitiveKind::ObjectOrNonScalar);
        }
        if matches!(root, "options" | "site_options") {
            return match resolve_field_node_and_tail(root, &attrs, scopes, policy) {
                Some((node, tail)) => classify_node_and_tail(node, tail),
                None => Some(BlockSensitiveKind::ObjectOrNonScalar),
            };
        }
        if let Some(UrlSourceKind::TrustedLink) = scopes.lookup(root) {
            return classify_trusted_link_tail(&attrs);
        }
        if let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy) {
            return classify_node_and_tail(node, tail);
        }
        if scopes.lookup(root).is_some() {
            return Some(BlockSensitiveKind::ObjectOrNonScalar);
        }
    }
    None
}

pub(crate) fn block_expr_touches_sensitive(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    if let Some(attrs) = resolve_menu_dot_chain(expr) {
        return !matches!(attrs.as_slice(), [_slot, "title" | "menu_key" | "menu_id"]);
    }
    if let Some((subscript, attrs)) = resolve_menu_get_item_subscript(expr) {
        return block_expr_touches_sensitive(subscript, scopes, policy)
            || !matches!(attrs.as_slice(), ["title" | "menu_key" | "menu_id"]);
    }
    if let Some((root, attrs)) = extract_attr_chain(expr) {
        if matches!(root, "options" | "site_options") {
            return match resolve_field_node_and_tail(root, &attrs, scopes, policy) {
                Some((node, tail)) => classify_node_and_tail(node, tail).is_some(),
                None => true,
            };
        }
        if root == "theme_options" || root == "menus" {
            return true;
        }
        if let Some(UrlSourceKind::TrustedLink) = scopes.lookup(root) {
            return classify_trusted_link_tail(&attrs).is_some();
        }
        if let Some((node, tail)) = resolve_field_node_and_tail(root, &attrs, scopes, policy) {
            return classify_node_and_tail(node, tail).is_some();
        }
        if scopes.lookup(root).is_some() {
            return true;
        }
    }
    if classify_block_sensitive_expr(expr, scopes, policy).is_some() {
        return true;
    }
    match expr {
        Expr::Var(_) | Expr::Const(_) => false,
        Expr::Slice(s) => {
            block_expr_touches_sensitive(&s.expr, scopes, policy)
                || s.start
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || s.stop
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || s.step
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::UnaryOp(u) => block_expr_touches_sensitive(&u.expr, scopes, policy),
        Expr::BinOp(b) => {
            block_expr_touches_sensitive(&b.left, scopes, policy)
                || block_expr_touches_sensitive(&b.right, scopes, policy)
        }
        Expr::Compare(c) => {
            block_expr_touches_sensitive(&c.expr, scopes, policy)
                || c.ops
                    .iter()
                    .any(|op| block_expr_touches_sensitive(&op.expr, scopes, policy))
        }
        Expr::IfExpr(i) => {
            block_expr_touches_sensitive(&i.test_expr, scopes, policy)
                || block_expr_touches_sensitive(&i.true_expr, scopes, policy)
                || i.false_expr
                    .as_ref()
                    .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::GetAttr(g) => block_expr_touches_sensitive(&g.expr, scopes, policy),
        Expr::GetItem(g) => {
            block_expr_touches_sensitive(&g.expr, scopes, policy)
                || block_expr_touches_sensitive(&g.subscript_expr, scopes, policy)
        }
        Expr::List(l) => l
            .items
            .iter()
            .any(|e| block_expr_touches_sensitive(e, scopes, policy)),
        Expr::Tuple(t) => t
            .items
            .iter()
            .any(|e| block_expr_touches_sensitive(e, scopes, policy)),
        Expr::Map(m) => {
            m.keys
                .iter()
                .any(|e| block_expr_touches_sensitive(e, scopes, policy))
                || m.values
                    .iter()
                    .any(|e| block_expr_touches_sensitive(e, scopes, policy))
        }
        Expr::Filter(f) => {
            f.expr
                .as_ref()
                .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || f.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
        }
        Expr::Test(t) => {
            block_expr_touches_sensitive(&t.expr, scopes, policy)
                || t.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
        }
        Expr::Call(_) => true,
    }
}

pub(crate) fn block_call_arg_touches_sensitive(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            block_expr_touches_sensitive(e, scopes, policy)
        }
    }
}

pub(crate) fn check_block_expr_restrictions(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    validate_expr(expr)?;
    match expr {
        Expr::Var(v) => {
            if v.id == "theme_options" {
                return Err("Template block không được truy cập theme_options.");
            }
            Ok(())
        }
        Expr::Const(_) => Ok(()),
        Expr::GetItem(g) => {
            let is_menus_lookup = matches!(&g.expr, Expr::Var(v) if v.id == "menus");
            if !is_menus_lookup {
                return Err("C-SEC cấm truy cập GetItem ([...]) trong template block.");
            }
            check_block_expr_restrictions(&g.subscript_expr, scopes, policy)?;
            if block_expr_touches_sensitive(&g.subscript_expr, scopes, policy) {
                return Err("Khóa tra cứu menus[...] phải là biểu thức scalar an toàn.");
            }
            Ok(())
        }
        Expr::Filter(f) => {
            if f.expr
                .as_ref()
                .is_some_and(|e| block_expr_touches_sensitive(e, scopes, policy))
                || f.args
                    .iter()
                    .any(|arg| block_call_arg_touches_sensitive(arg, scopes, policy))
            {
                return Err("C-SEC cấm áp dụng filter lên biến richtext/url/media/repeater.");
            }
            if let Some(e) = &f.expr {
                check_block_expr_restrictions(e, scopes, policy)?;
            }
            for arg in &f.args {
                check_block_call_arg_restrictions(arg, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Slice(s) => {
            if block_expr_touches_sensitive(&s.expr, scopes, policy) {
                return Err("C-SEC cấm cắt lát (slice) trên biến nhạy cảm.");
            }
            check_block_expr_restrictions(&s.expr, scopes, policy)?;
            if let Some(start) = &s.start {
                check_block_expr_restrictions(start, scopes, policy)?;
            }
            if let Some(stop) = &s.stop {
                check_block_expr_restrictions(stop, scopes, policy)?;
            }
            if let Some(step) = &s.step {
                check_block_expr_restrictions(step, scopes, policy)?;
            }
            Ok(())
        }
        Expr::UnaryOp(u) => check_block_expr_restrictions(&u.expr, scopes, policy),
        Expr::BinOp(b) => {
            check_block_expr_restrictions(&b.left, scopes, policy)?;
            check_block_expr_restrictions(&b.right, scopes, policy)
        }
        Expr::Compare(c) => {
            check_block_expr_restrictions(&c.expr, scopes, policy)?;
            for op in &c.ops {
                check_block_expr_restrictions(&op.expr, scopes, policy)?;
            }
            Ok(())
        }
        Expr::IfExpr(i) => {
            check_block_expr_restrictions(&i.test_expr, scopes, policy)?;
            check_block_expr_restrictions(&i.true_expr, scopes, policy)?;
            if let Some(f) = &i.false_expr {
                check_block_expr_restrictions(f, scopes, policy)?;
            }
            Ok(())
        }
        Expr::GetAttr(g) => check_block_expr_restrictions(&g.expr, scopes, policy),
        Expr::List(l) => {
            for item in &l.items {
                check_block_expr_restrictions(item, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                check_block_expr_restrictions(item, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                check_block_expr_restrictions(k, scopes, policy)?;
                check_block_expr_restrictions(v, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Test(t) => {
            check_block_expr_restrictions(&t.expr, scopes, policy)?;
            for arg in &t.args {
                check_block_call_arg_restrictions(arg, scopes, policy)?;
            }
            Ok(())
        }
        Expr::Call(_) => Err("C-SEC cấm lời gọi hàm Call trong template block."),
    }
}

pub(crate) fn check_block_call_arg_restrictions(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => {
            check_block_expr_restrictions(e, scopes, policy)
        }
    }
}

pub(crate) fn is_valid_block_url_expr(
    expr: &Expr<'_>,
    slot: UrlAttrSlot,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match classify_block_sensitive_expr(expr, scopes, policy) {
        Some(BlockSensitiveKind::HrefUrl) => {
            matches!(slot, UrlAttrSlot::AnchorHref | UrlAttrSlot::Cite)
        }
        Some(BlockSensitiveKind::MediaUrl) => {
            matches!(slot, UrlAttrSlot::AnchorHref | UrlAttrSlot::SrcAndHref)
        }
        _ => false,
    }
}

pub(crate) fn resolve_block_collection_source(
    expr: &Expr<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Option<UrlSourceKind> {
    if let Some((root, attrs)) = extract_attr_chain(expr)
        && let Some((node, &[])) = resolve_field_node_and_tail(root, &attrs, scopes, policy)
    {
        return match node {
            FieldTemplateNode::Repeater(fields) => Some(UrlSourceKind::FieldNode(
                FieldTemplateNode::Group(fields.clone()),
            )),
            FieldTemplateNode::Gallery => Some(UrlSourceKind::FieldNode(FieldTemplateNode::Media)),
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
    if let Some((_sub, attrs)) = resolve_menu_get_item_subscript(expr)
        && attrs.as_slice() == ["items"]
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    if let Some(attrs) = resolve_menu_dot_chain(expr)
        && matches!(attrs.as_slice(), [_slot, "items"])
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    if let Expr::GetAttr(g) = expr
        && g.name == "children"
        && let Expr::Var(v) = &g.expr
        && scopes.lookup(v.id) == Some(&UrlSourceKind::TrustedLink)
    {
        return Some(UrlSourceKind::TrustedLink);
    }
    None
}

pub(crate) fn walk_block_ast_sequence(
    children: &[Stmt<'_>],
    mut states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    for child in children {
        let (next_states, next_scopes) = walk_block_ast_stmt(child, states, scopes, policy)?;
        states = next_states;
        scopes = next_scopes;
        if states.len() > MAX_THEME_WALKER_STATES {
            return Err("Quá nhiều trạng thái HTML trong template block.");
        }
    }
    Ok((states, scopes))
}
