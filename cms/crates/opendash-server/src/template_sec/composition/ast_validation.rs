use super::*;

pub(crate) fn validate_stmt(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    validate_stmt_with_icons(stmt, false, false)
}

pub(crate) fn validate_stmt_with_icons(
    stmt: &Stmt<'_>,
    allow_od_icon: bool,
    in_set_block: bool,
) -> Result<(), &'static str> {
    validate_stmt_with_icons_ext(stmt, allow_od_icon, false, in_set_block)
}

pub(crate) const RESERVED_CONTEXT_VARS: &[&str] = &[
    "od_icon",
    "od_image",
    "od_hook",
    "options",
    "site_options",
    "theme_options",
    "custom_options",
    "theme_assets",
    "site",
    "menus",
    "entries",
    "pagination",
    "breadcrumbs",
    "taxonomy",
    "search",
    "related_entries",
    "recent_posts",
    "route",
    "locale",
    "t",
    "content_type",
    "theme_asset_url",
    "previous_url",
    "next_url",
    "archive_url",
    "od_content",
    "od_part",
];

pub(crate) fn binds_reserved_context(expr: &Expr<'_>) -> bool {
    binds_reserved_context_for(expr, false)
}

pub(crate) fn binds_reserved_context_for(expr: &Expr<'_>, is_taxonomy_template: bool) -> bool {
    match expr {
        Expr::Var(var) => {
            RESERVED_CONTEXT_VARS.contains(&var.id) || (is_taxonomy_template && var.id == "term")
        }
        Expr::Tuple(tuple) => tuple
            .items
            .iter()
            .any(|item| binds_reserved_context_for(item, is_taxonomy_template)),
        Expr::List(list) => list
            .items
            .iter()
            .any(|item| binds_reserved_context_for(item, is_taxonomy_template)),
        // Attribute and subscript assignments are forbidden rather than
        // allowing a mutable binding to masquerade as a reserved context root.
        Expr::GetAttr(attr) => binds_reserved_context_for(&attr.expr, is_taxonomy_template),
        Expr::GetItem(item) => binds_reserved_context_for(&item.expr, is_taxonomy_template),
        _ => false,
    }
}

pub(crate) fn reject_reserved_context_rebinding(stmt: &Stmt<'_>) -> Result<(), &'static str> {
    reject_reserved_context_rebinding_for(stmt, false)
}

pub(crate) fn reject_reserved_context_rebinding_for(
    stmt: &Stmt<'_>,
    is_taxonomy_template: bool,
) -> Result<(), &'static str> {
    let forbidden = || Err("C-SEC cấm gán lại biến ngữ cảnh hệ thống trong theme.");
    match stmt {
        Stmt::Template(t) => t
            .children
            .iter()
            .try_for_each(|s| reject_reserved_context_rebinding_for(s, is_taxonomy_template)),
        Stmt::ForLoop(f) => {
            if binds_reserved_context_for(&f.target, is_taxonomy_template) {
                return forbidden();
            }
            f.body
                .iter()
                .chain(&f.else_body)
                .try_for_each(|s| reject_reserved_context_rebinding_for(s, is_taxonomy_template))
        }
        Stmt::WithBlock(w) => {
            if w.assignments
                .iter()
                .any(|(key, _)| binds_reserved_context_for(key, is_taxonomy_template))
            {
                return forbidden();
            }
            w.body
                .iter()
                .try_for_each(|s| reject_reserved_context_rebinding_for(s, is_taxonomy_template))
        }
        Stmt::Set(s) => {
            if binds_reserved_context_for(&s.target, is_taxonomy_template) {
                forbidden()
            } else {
                Ok(())
            }
        }
        Stmt::SetBlock(sb) => {
            if binds_reserved_context_for(&sb.target, is_taxonomy_template) {
                return forbidden();
            }
            sb.body
                .iter()
                .try_for_each(|s| reject_reserved_context_rebinding_for(s, is_taxonomy_template))
        }
        Stmt::IfCond(i) => i
            .true_body
            .iter()
            .chain(&i.false_body)
            .try_for_each(|s| reject_reserved_context_rebinding_for(s, is_taxonomy_template)),
        _ => Ok(()),
    }
}

pub(crate) fn validate_expr(expr: &Expr<'_>) -> Result<(), &'static str> {
    match expr {
        Expr::Var(_) | Expr::Const(_) => Ok(()),
        Expr::Slice(s) => {
            validate_expr(&s.expr)?;
            if let Some(start) = &s.start {
                validate_expr(start)?;
            }
            if let Some(stop) = &s.stop {
                validate_expr(stop)?;
            }
            if let Some(step) = &s.step {
                validate_expr(step)?;
            }
            Ok(())
        }
        Expr::UnaryOp(u) => validate_expr(&u.expr),
        Expr::BinOp(b) => {
            validate_expr(&b.left)?;
            validate_expr(&b.right)
        }
        Expr::Compare(c) => {
            validate_expr(&c.expr)?;
            for op in &c.ops {
                validate_expr(&op.expr)?;
            }
            Ok(())
        }
        Expr::IfExpr(i) => {
            validate_expr(&i.test_expr)?;
            validate_expr(&i.true_expr)?;
            if let Some(f) = &i.false_expr {
                validate_expr(f)?;
            }
            Ok(())
        }
        Expr::GetAttr(g) => {
            if g.name.starts_with("__") {
                return Err("C-SEC cấm truy cập thuộc tính dunder (__).");
            }
            validate_expr(&g.expr)
        }
        Expr::GetItem(g) => {
            if matches!(&g.expr, Expr::Var(v) if v.id == "t") {
                return Err("C-SEC cấm truy cập t[...] động; hãy dùng t.<key>.");
            }
            if matches!(&g.subscript_expr, Expr::Const(c) if c.value.as_str().is_some_and(|s| s.starts_with("__")))
            {
                return Err("C-SEC cấm truy cập thuộc tính dunder (__).");
            }
            validate_expr(&g.expr)?;
            validate_expr(&g.subscript_expr)
        }
        Expr::List(l) => {
            for item in &l.items {
                validate_expr(item)?;
            }
            Ok(())
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                validate_expr(item)?;
            }
            Ok(())
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            Ok(())
        }
        Expr::Filter(f) => {
            if !ALLOWED_FILTERS.contains(&f.name) {
                return Err("Template sử dụng filter MiniJinja không nằm trong whitelist C-SEC.");
            }
            if let Some(e) = &f.expr {
                validate_expr(e)?;
            }
            for arg in &f.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        Expr::Test(t) => {
            if !ALLOWED_TESTS.contains(&t.name) {
                return Err("Template sử dụng test MiniJinja không nằm trong whitelist C-SEC.");
            }
            validate_expr(&t.expr)?;
            for arg in &t.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        Expr::Call(c) => validate_l10n_call(c),
    }
}

pub(crate) fn is_l10n_call_expr(expr: &Expr<'_>) -> bool {
    matches!(
        expr,
        Expr::Call(c) if matches!(&c.expr, Expr::Var(v) if matches!(v.id, "__" | "_x" | "_n"))
    )
}

pub(crate) fn validate_l10n_call(
    call: &minijinja::machinery::ast::Call<'_>,
) -> Result<(), &'static str> {
    let Expr::Var(v) = &call.expr else {
        return Err("C-SEC cấm lời gọi hàm Call trong template.");
    };
    let is_const_str = |arg: &minijinja::machinery::ast::CallArg<'_>| -> bool {
        matches!(
            arg,
            minijinja::machinery::ast::CallArg::Pos(Expr::Const(c)) if c.value.as_str().is_some()
        )
    };
    let is_static_domain_or_params = |arg: &minijinja::machinery::ast::CallArg<'_>| -> bool {
        is_const_str(arg) || matches!(arg, minijinja::machinery::ast::CallArg::Pos(Expr::Map(_)))
    };
    match v.id {
        "__" => {
            if call.args.is_empty() || !is_const_str(&call.args[0]) {
                return Err("__() yêu cầu msgid là chuỗi literal tĩnh.");
            }
            if let Some(arg1) = call.args.get(1)
                && !is_static_domain_or_params(arg1)
            {
                return Err("__() domain phải là chuỗi literal tĩnh.");
            }
            for arg in &call.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        "_x" => {
            if call.args.len() < 2 || !is_const_str(&call.args[0]) || !is_const_str(&call.args[1]) {
                return Err("_x() yêu cầu msgid và context là chuỗi literal tĩnh.");
            }
            if let Some(arg2) = call.args.get(2)
                && !is_static_domain_or_params(arg2)
            {
                return Err("_x() domain phải là chuỗi literal tĩnh.");
            }
            for arg in &call.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        "_n" => {
            if call.args.len() < 3 || !is_const_str(&call.args[0]) || !is_const_str(&call.args[1]) {
                return Err("_n() yêu cầu singular và plural là chuỗi literal tĩnh.");
            }
            if let Some(arg3) = call.args.get(3)
                && !is_static_domain_or_params(arg3)
            {
                return Err("_n() domain phải là chuỗi literal tĩnh.");
            }
            for arg in &call.args {
                validate_call_arg(arg)?;
            }
            Ok(())
        }
        _ => Err("C-SEC cấm lời gọi hàm Call trong template."),
    }
}

pub(crate) fn validate_call_arg(
    arg: &minijinja::machinery::ast::CallArg<'_>,
) -> Result<(), &'static str> {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => validate_expr(e),
    }
}

/// Collects every `t.<key>` translation key referenced by a theme template AST.
pub fn collect_theme_translation_keys(source: &str) -> Result<BTreeSet<String>, &'static str> {
    let ast = parse(source, "theme-l10n", SyntaxConfig::default())
        .map_err(|_| "Cú pháp theme MiniJinja không hợp lệ.")?;
    let mut keys = BTreeSet::new();
    collect_t_keys_stmt(&ast, &mut keys);
    Ok(keys)
}

pub(crate) fn collect_t_keys_expr(expr: &Expr<'_>, out: &mut BTreeSet<String>) {
    match expr {
        Expr::GetAttr(g) => {
            if matches!(&g.expr, Expr::Var(v) if v.id == "t") {
                out.insert(g.name.to_owned());
            } else {
                collect_t_keys_expr(&g.expr, out);
            }
        }
        Expr::GetItem(g) => {
            collect_t_keys_expr(&g.expr, out);
            collect_t_keys_expr(&g.subscript_expr, out);
        }
        Expr::Slice(s) => {
            collect_t_keys_expr(&s.expr, out);
            if let Some(start) = &s.start {
                collect_t_keys_expr(start, out);
            }
            if let Some(stop) = &s.stop {
                collect_t_keys_expr(stop, out);
            }
            if let Some(step) = &s.step {
                collect_t_keys_expr(step, out);
            }
        }
        Expr::UnaryOp(u) => collect_t_keys_expr(&u.expr, out),
        Expr::BinOp(b) => {
            collect_t_keys_expr(&b.left, out);
            collect_t_keys_expr(&b.right, out);
        }
        Expr::Compare(c) => {
            collect_t_keys_expr(&c.expr, out);
            for op in &c.ops {
                collect_t_keys_expr(&op.expr, out);
            }
        }
        Expr::IfExpr(i) => {
            collect_t_keys_expr(&i.test_expr, out);
            collect_t_keys_expr(&i.true_expr, out);
            if let Some(f) = &i.false_expr {
                collect_t_keys_expr(f, out);
            }
        }
        Expr::List(l) => {
            for item in &l.items {
                collect_t_keys_expr(item, out);
            }
        }
        Expr::Tuple(t) => {
            for item in &t.items {
                collect_t_keys_expr(item, out);
            }
        }
        Expr::Map(m) => {
            for (k, v) in m.keys.iter().zip(m.values.iter()) {
                collect_t_keys_expr(k, out);
                collect_t_keys_expr(v, out);
            }
        }
        Expr::Filter(f) => {
            if let Some(e) = &f.expr {
                collect_t_keys_expr(e, out);
            }
            for arg in &f.args {
                collect_t_keys_call_arg(arg, out);
            }
        }
        Expr::Test(t) => {
            collect_t_keys_expr(&t.expr, out);
            for arg in &t.args {
                collect_t_keys_call_arg(arg, out);
            }
        }
        Expr::Call(call) => {
            for arg in &call.args {
                collect_t_keys_call_arg(arg, out);
            }
        }
        Expr::Var(_) | Expr::Const(_) => {}
    }
}

pub(crate) fn collect_t_keys_call_arg(
    arg: &minijinja::machinery::ast::CallArg<'_>,
    out: &mut BTreeSet<String>,
) {
    match arg {
        minijinja::machinery::ast::CallArg::Pos(e)
        | minijinja::machinery::ast::CallArg::Kwarg(_, e)
        | minijinja::machinery::ast::CallArg::PosSplat(e)
        | minijinja::machinery::ast::CallArg::KwargSplat(e) => collect_t_keys_expr(e, out),
    }
}
