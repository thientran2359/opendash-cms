use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BlockSensitiveKind {
    Richtext,
    HrefUrl,
    MediaUrl,
    ObjectOrNonScalar,
}

pub(super) fn resolve_menu_get_item_subscript<'a>(
    expr: &'a Expr<'a>,
) -> Option<(&'a Expr<'a>, Vec<&'a str>)> {
    let mut attrs = Vec::new();
    let mut cur = expr;
    while let Expr::GetAttr(g) = cur {
        attrs.push(g.name);
        cur = &g.expr;
    }
    let Expr::GetItem(gi) = cur else {
        return None;
    };
    let Expr::Var(v) = &gi.expr else {
        return None;
    };
    if v.id != "menus" {
        return None;
    }
    attrs.reverse();
    Some((&gi.subscript_expr, attrs))
}

pub(super) fn resolve_menu_dot_chain<'a>(expr: &'a Expr<'a>) -> Option<Vec<&'a str>> {
    let (root, attrs) = extract_attr_chain(expr)?;
    if root != "menus" || attrs.is_empty() {
        return None;
    }
    Some(attrs)
}

pub(super) fn classify_trusted_link_tail(tail: &[&str]) -> Option<BlockSensitiveKind> {
    match tail {
        ["url"] => Some(BlockSensitiveKind::HrefUrl),
        ["label" | "id" | "is_active" | "is_ancestor" | "is_external" | "depth"] => None,
        _ => Some(BlockSensitiveKind::ObjectOrNonScalar),
    }
}
