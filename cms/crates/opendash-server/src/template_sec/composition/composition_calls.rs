use super::*;

pub(crate) fn valid_part_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && !key.contains("..")
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'/'))
}

pub(crate) fn check_part_graph(
    key: &str,
    sites: &BTreeMap<String, Vec<CompositionSite>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
    expansions: &mut usize,
) -> Result<(), &'static str> {
    if depth > MAX_COMPOSITION_PART_DEPTH {
        return Err("Độ sâu od_part vượt giới hạn 3.");
    }
    if !visiting.insert(key.to_owned()) {
        return Err("Phát hiện vòng lặp od_part.");
    }
    *expansions += 1;
    if *expansions > MAX_COMPOSITION_EXPANSIONS {
        return Err("od_part vượt quá 32 lần mở rộng.");
    }
    let node = sites
        .get(key)
        .ok_or("od_part trỏ tới part không tồn tại.")?;
    for site in node {
        if let CompositionCall::Part(child) = &site.call {
            check_part_graph(child, sites, visiting, depth + 1, expansions)?;
        }
    }
    visiting.remove(key);
    Ok(())
}

pub(crate) fn parse_composition_call(
    expr: &Expr<'_>,
) -> Result<Option<CompositionCall>, &'static str> {
    let Expr::Call(call) = expr else {
        validate_expr(expr)?;
        return Ok(None);
    };
    match &call.expr {
        Expr::Var(var) if var.id == "od_content" => {
            if !call.args.is_empty() {
                return Err("od_content() không nhận tham số.");
            }
            Ok(Some(CompositionCall::Content))
        }
        Expr::Var(var) if var.id == "od_part" => {
            let [minijinja::machinery::ast::CallArg::Pos(Expr::Const(value))] =
                call.args.as_slice()
            else {
                return Err("od_part() chỉ nhận một string literal.");
            };
            let key = value
                .value
                .as_str()
                .ok_or("od_part() phải dùng string literal.")?;
            if !valid_part_key(key) {
                return Err("od_part key không hợp lệ.");
            }
            Ok(Some(CompositionCall::Part(key.to_owned())))
        }
        _ => Err("C-SEC cấm lời gọi hàm Call ngoài od_part/od_content."),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum OdIconNameArg<'a> {
    Literal(&'a str),
    FieldNamePath(&'a str, Vec<&'a str>),
}

pub(crate) fn parse_od_icon_call<'a>(
    expr: &'a Expr<'_>,
) -> Result<Option<(&'a str, Option<&'a str>)>, &'static str> {
    match parse_od_icon_call_ext(expr, false)? {
        Some((OdIconNameArg::Literal(name), class)) => Ok(Some((name, class))),
        Some((OdIconNameArg::FieldNamePath(_, _), _)) => {
            Err("od_icon() yêu cầu string literal name và class tùy chọn.")
        }
        None => Ok(None),
    }
}

pub(crate) fn parse_od_hook_call<'a>(
    expr: &'a Expr<'_>,
) -> Result<Option<(&'a str, Option<&'a Expr<'a>>)>, &'static str> {
    let Expr::Call(call) = expr else {
        return Ok(None);
    };
    let Expr::Var(var) = &call.expr else {
        return Ok(None);
    };
    if var.id != "od_hook" {
        return Ok(None);
    }
    let [
        minijinja::machinery::ast::CallArg::Pos(Expr::Const(slot)),
        rest @ ..,
    ] = call.args.as_slice()
    else {
        return Err("od_hook() yêu cầu string literal slot và biểu thức tham số tùy chọn.");
    };
    let slot = slot
        .value
        .as_str()
        .ok_or("od_hook() slot phải là string literal.")?;
    if !valid_hook_slot_name(slot) {
        return Err("od_hook() tên slot không hợp lệ.");
    }
    let arg_expr = match rest {
        [] => None,
        [minijinja::machinery::ast::CallArg::Pos(arg)] => {
            validate_expr(arg)?;
            Some(arg)
        }
        _ => return Err("od_hook() chỉ nhận slot string literal và tối đa một tham số tùy chọn."),
    };
    Ok(Some((slot, arg_expr)))
}

pub(crate) fn reject_icons_in_set_block(children: &[Stmt<'_>]) -> Result<(), &'static str> {
    fn contains_icon(stmt: &Stmt<'_>) -> bool {
        match stmt {
            Stmt::EmitExpr(e) => {
                parse_od_icon_call_ext(&e.expr, true).is_ok_and(|v| v.is_some())
                    || parse_od_image_call(&e.expr).is_ok_and(|v| v.is_some())
                    || parse_od_hook_call(&e.expr).is_ok_and(|v| v.is_some())
            }
            Stmt::IfCond(c) => c.true_body.iter().chain(&c.false_body).any(contains_icon),
            Stmt::ForLoop(c) => c.body.iter().chain(&c.else_body).any(contains_icon),
            Stmt::WithBlock(c) => c.body.iter().any(contains_icon),
            Stmt::SetBlock(c) => c.body.iter().any(contains_icon),
            Stmt::Template(c) => c.children.iter().any(contains_icon),
            _ => false,
        }
    }
    if children.iter().any(contains_icon) {
        Err("C-SEC cấm od_icon() trong set block.")
    } else {
        Ok(())
    }
}
