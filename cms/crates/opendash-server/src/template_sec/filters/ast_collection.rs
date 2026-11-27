use super::*;

pub(crate) fn collect_composition_sites(
    stmt: &Stmt<'_>,
    source: &str,
    in_for_or_set_block: bool,
    sites: &mut Vec<CompositionSite>,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::EmitExpr(e) => {
            if parse_od_icon_call_ext(&e.expr, true)?.is_some()
                || parse_od_image_call(&e.expr)?.is_some()
                || parse_od_hook_call(&e.expr)?.is_some()
            {
                return Ok(());
            }
            let Some(call) = parse_composition_call(&e.expr)? else {
                return Ok(());
            };
            if in_for_or_set_block {
                return Err("od_part/od_content không được nằm trong for hoặc set block.");
            }
            let start = e.span().start_offset as usize;
            let expr_end = e.span().end_offset as usize;
            if start >= source.len()
                || expr_end > source.len()
                || !source[start..].starts_with("{{")
            {
                return Err("od_part/od_content chỉ được xuất hiện ở phần thân HTML.");
            }
            // The AST span covers '{{' through the final expression token,
            // but not the closing '}}'. Search only after that token so a
            // quoted string containing '}}' cannot truncate the replacement.
            let suffix = source
                .get(expr_end..)
                .ok_or("Vị trí biểu thức composition không hợp lệ.")?;
            let close = suffix
                .find("}}")
                .ok_or("od_part/od_content thiếu dấu đóng.")?;
            if !suffix[..close]
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '-')
            {
                return Err("Biểu thức composition chứa token ngoài allowlist.");
            }
            sites.push(CompositionSite {
                start,
                end: expr_end + close + 2,
                call,
            });
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(expr) = &f.filter_expr {
                validate_expr(expr)?;
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::WithBlock(w) => {
            for (key, val) in &w.assignments {
                validate_expr(key)?;
                validate_expr(val)?;
            }
            for child in &w.body {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        _ => validate_stmt(stmt)?,
    }
    Ok(())
}

pub(crate) fn validate_stmt_with_icons_ext(
    stmt: &Stmt<'_>,
    allow_od_icon: bool,
    allow_dynamic_icon: bool,
    in_set_block: bool,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::EmitRaw(_) => Ok(()),
        Stmt::EmitExpr(e) => {
            if allow_od_icon
                && !in_set_block
                && (parse_od_icon_call_ext(&e.expr, allow_dynamic_icon)?.is_some()
                    || parse_od_image_call(&e.expr)?.is_some()
                    || parse_od_hook_call(&e.expr)?.is_some())
            {
                Ok(())
            } else {
                validate_expr(&e.expr)
            }
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
            }
            for child in &f.body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            for child in &f.else_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in &i.true_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            for child in &i.false_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::WithBlock(w) => {
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            for child in &w.body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::Set(s) => {
            validate_expr(&s.target)?;
            validate_expr(&s.expr)
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                validate_stmt_with_icons_ext(child, allow_od_icon, allow_dynamic_icon, true)?;
            }
            Ok(())
        }
        Stmt::AutoEscape(_)
        | Stmt::FilterBlock(_)
        | Stmt::Macro(_)
        | Stmt::CallBlock(_)
        | Stmt::Import(_)
        | Stmt::FromImport(_)
        | Stmt::Extends(_)
        | Stmt::Include(_)
        | Stmt::Block(_)
        | Stmt::Do(_) => Err("Template sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

pub(crate) fn collect_t_keys_stmt(stmt: &Stmt<'_>, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::EmitExpr(e) => collect_t_keys_expr(&e.expr, out),
        Stmt::ForLoop(f) => {
            collect_t_keys_expr(&f.iter, out);
            if let Some(cond) = &f.filter_expr {
                collect_t_keys_expr(cond, out);
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::IfCond(i) => {
            collect_t_keys_expr(&i.expr, out);
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::WithBlock(w) => {
            for (_, val) in &w.assignments {
                collect_t_keys_expr(val, out);
            }
            for child in &w.body {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::Set(s) => collect_t_keys_expr(&s.expr, out),
        Stmt::SetBlock(sb) => {
            if let Some(filter) = &sb.filter {
                collect_t_keys_expr(filter, out);
            }
            for child in &sb.body {
                collect_t_keys_stmt(child, out);
            }
        }
        _ => {}
    }
}
