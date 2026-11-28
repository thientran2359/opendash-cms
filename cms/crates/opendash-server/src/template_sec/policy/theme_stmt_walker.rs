use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_theme_stmt_with_url_scopes(
    stmt: &Stmt<'_>,
    states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    is_part: bool,
    is_full_doctype_doc: bool,
    is_taxonomy_template: bool,
    policy: &ThemeUrlPolicy,
    allow_trusted_theme_markup: bool,
    allow_search_form: bool,
    usage: &mut ThemeTemplateUsageFlags,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    match stmt {
        Stmt::Template(t) => walk_theme_stmts_with_url_scopes(
            &t.children,
            states,
            scopes,
            is_part,
            is_full_doctype_doc,
            is_taxonomy_template,
            policy,
            allow_trusted_theme_markup,
            allow_search_form,
            usage,
        ),
        Stmt::EmitRaw(raw) => {
            let mut next_states = BTreeSet::new();
            for initial in states {
                let mut cur = initial;
                for ch in raw.raw.chars() {
                    let depths = cur.depths();
                    cur = step_theme_raw_char(
                        cur,
                        ch,
                        HtmlStepOptions {
                            is_part,
                            is_full_doctype_doc,
                            allow_trusted_theme_markup,
                            allow_search_form,
                            forbid_style_tag: false,
                        },
                        depths,
                    )?;
                }
                next_states.insert(cur);
            }
            if next_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML trong theme template.");
            }
            Ok((next_states, scopes))
        }
        Stmt::EmitExpr(expr) => {
            reject_style_control_flow(&states)?;
            if let Some((icon_arg, _)) = parse_od_icon_call_ext(&expr.expr, true)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML.");
                }
                if let OdIconNameArg::FieldNamePath(root, attrs) = icon_arg {
                    let resolved = resolve_field_node_and_tail(
                        root,
                        &attrs[..attrs.len() - 1],
                        &scopes,
                        policy,
                    );
                    if !matches!(resolved, Some((FieldTemplateNode::Icon, &[]))) {
                        return Err("od_icon() động phải tham chiếu trường kiểu icon.");
                    }
                }
                return Ok((states, scopes));
            }
            if let Some(img_call) = parse_od_image_call(&expr.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_image() chỉ được xuất trong thân HTML.");
                }
                if !is_valid_od_image_media_target(
                    img_call.media_root,
                    &img_call.media_attrs,
                    &scopes,
                    policy,
                ) {
                    return Err("od_image() phải tham chiếu trường media hợp lệ.");
                }
                if let Some(alt_expr) = img_call.alt_expr {
                    record_usage_flags_in_expr(alt_expr, usage);
                }
                return Ok((states, scopes));
            }
            if let Some((_, arg_expr)) = parse_od_hook_call(&expr.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_hook() chỉ được xuất trong thân HTML.");
                }
                if let Some(arg) = arg_expr {
                    record_usage_flags_in_expr(arg, usage);
                }
                return Ok((states, scopes));
            }
            if parse_composition_call(&expr.expr)?.is_some() {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_part/od_content chỉ được xuất hiện trong thân HTML.");
                }
                if states.iter().any(|s| s.form_depth() > 0) {
                    return Err("C-SEC cấm gọi od_part/od_content bên trong <form>.");
                }
                return Ok((states, scopes));
            }
            record_usage_flags_in_expr(&expr.expr, usage);
            if is_richtext_field_expr(&expr.expr, &scopes, policy)
                && states.iter().any(|s| s.inner() != &ThemeHtmlState::Body)
            {
                return Err("Trường richtext chỉ được xuất trực tiếp trong phần thân HTML.");
            }
            if states.iter().any(|s| {
                matches!(
                    s.inner(),
                    ThemeHtmlState::Style { .. }
                        | ThemeHtmlState::Script { .. }
                        | ThemeHtmlState::Comment(_)
                        | ThemeHtmlState::BodyLt
                        | ThemeHtmlState::BodyLtSlash
                        | ThemeHtmlState::BodyLtBang { .. }
                        | ThemeHtmlState::Doctype
                        | ThemeHtmlState::Title {
                            closing_prefix: 1..
                        }
                )
            }) {
                return Err(
                    "C-SEC cấm nội suy Jinja ở comment, <style>, DOCTYPE hoặc biên thẻ HTML.",
                );
            }
            let has_non_tag = states
                .iter()
                .any(|s| !matches!(s.inner(), ThemeHtmlState::Tag { .. }));
            let has_tag = states
                .iter()
                .any(|s| matches!(s.inner(), ThemeHtmlState::Tag { .. }));
            if has_non_tag && has_tag {
                return Err("C-SEC cấm trộn lẫn ngữ cảnh thẻ HTML và thân HTML tại điểm nội suy.");
            }
            if !has_tag {
                return Ok((states, scopes));
            }

            let mut expected_url_slot: Option<Option<UrlAttrSlot>> = None;
            let mut updated_states = BTreeSet::new();
            for state in states {
                let track_depths = allow_trusted_theme_markup || allow_search_form;
                let depths = state.depths();
                let ThemeHtmlState::Tag {
                    tag_name,
                    is_close,
                    phase,
                    attr_name,
                    quote,
                    attr_has_static_bytes,
                    mut attr_expr_count,
                    seen_attrs,
                    link_rel_stylesheet,
                    mut link_has_valid_href,
                    meta_ok,
                    mut form_has_action,
                    attr_static_buf,
                } = state.into_inner()
                else {
                    unreachable!("non-tag state already filtered");
                };
                if is_close || phase != TagPhase::AttrValue || quote.is_none() {
                    return Err(
                        "C-SEC chỉ cho phép nội suy bên trong giá trị thuộc tính có dấu ngoặc kép.",
                    );
                }
                if forbids_dynamic_interpolation(&tag_name, &attr_name)
                    || matches!(attr_name.as_str(), "style" | "srcset" | "ping")
                    || (tag_name == "link" && attr_name == "rel")
                    || tag_name == "meta"
                {
                    return Err(
                        "C-SEC cấm nội suy bên trong thuộc tính bảo mật hoặc sự kiện HTML.",
                    );
                }
                if matches!(
                    attr_name.as_str(),
                    "href" | "src" | "poster" | "cite" | "action" | "formaction"
                ) {
                    if is_l10n_call_expr(&expr.expr) {
                        return Err("C-SEC cấm gọi __/_x/_n bên trong thuộc tính URL.");
                    }
                    if attr_has_static_bytes || attr_expr_count != 0 {
                        return Err(
                            "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                        );
                    }
                    let Some(slot) =
                        classify_theme_url_slot(&tag_name, &attr_name, allow_search_form)
                    else {
                        return Err("C-SEC cấm nội suy URL động trên thuộc tính hoặc thẻ này.");
                    };
                    if let Some(prev_slot) = expected_url_slot {
                        if prev_slot != Some(slot) {
                            return Err("Nhóm thuộc tính URL phải đồng nhất qua các nhánh rẽ.");
                        }
                    } else {
                        expected_url_slot = Some(Some(slot));
                    }
                    if !is_valid_theme_url_expr(&expr.expr, slot, &scopes, policy) {
                        return Err(
                            "URL động trong thuộc tính không nằm trong whitelist typed URL.",
                        );
                    }
                    attr_expr_count = 1;
                    if slot == UrlAttrSlot::LinkCss {
                        link_has_valid_href = true;
                    } else if slot == UrlAttrSlot::FormAction {
                        form_has_action = true;
                    }
                } else {
                    if let Some(prev_slot) = expected_url_slot {
                        if prev_slot.is_some() {
                            return Err(
                                "Không thể trộn thuộc tính URL và thuộc tính thường qua các nhánh.",
                            );
                        }
                    } else {
                        expected_url_slot = Some(None);
                    }
                    attr_expr_count = attr_expr_count.saturating_add(1);
                }
                updated_states.insert(ThemeHtmlState::with_depths(
                    ThemeHtmlState::Tag {
                        tag_name,
                        is_close,
                        phase,
                        attr_name,
                        quote,
                        attr_has_static_bytes,
                        attr_expr_count,
                        seen_attrs,
                        link_rel_stylesheet,
                        link_has_valid_href,
                        meta_ok,
                        form_has_action,
                        attr_static_buf,
                    },
                    track_depths,
                    depths,
                ));
            }
            Ok((updated_states, scopes))
        }
        Stmt::IfCond(cond) => {
            reject_style_control_flow(&states)?;
            validate_expr(&cond.expr)?;
            record_usage_flags_in_expr(&cond.expr, usage);
            let (mut true_states, true_scopes) = walk_theme_stmts_with_url_scopes(
                &cond.true_body,
                states.clone(),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                is_taxonomy_template,
                policy,
                allow_trusted_theme_markup,
                allow_search_form,
                usage,
            )?;
            let (false_states, false_scopes) = walk_theme_stmts_with_url_scopes(
                &cond.false_body,
                states,
                scopes,
                is_part,
                is_full_doctype_doc,
                is_taxonomy_template,
                policy,
                allow_trusted_theme_markup,
                allow_search_form,
                usage,
            )?;
            true_states.extend(false_states);
            if true_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML tại nhánh điều kiện.");
            }
            Ok((
                true_states,
                UrlScopeStack::intersect(&true_scopes, &false_scopes),
            ))
        }
        Stmt::WithBlock(w) => {
            reject_style_control_flow(&states)?;
            let mut frame = BTreeMap::new();
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
                record_usage_flags_in_expr(v, usage);
                for ident in extract_bound_idents(k)? {
                    if matches!(ident.as_str(), "entry" | "term" | "field") {
                        return Err("C-SEC cấm gán lại biến ngữ cảnh trong with.");
                    }
                    frame.insert(ident, None);
                }
            }
            scopes.push_frame(frame);
            let (out_states, mut out_scopes) = walk_theme_stmts_with_url_scopes(
                &w.body,
                states,
                scopes,
                is_part,
                is_full_doctype_doc,
                is_taxonomy_template,
                policy,
                allow_trusted_theme_markup,
                allow_search_form,
                usage,
            )?;
            out_scopes.pop_frame();
            Ok((out_states, out_scopes))
        }
        Stmt::Set(s) => {
            reject_style_control_flow(&states)?;
            validate_expr(&s.target)?;
            validate_expr(&s.expr)?;
            record_usage_flags_in_expr(&s.expr, usage);
            for ident in extract_bound_idents(&s.target)? {
                if matches!(ident.as_str(), "entry" | "term" | "field") {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh qua set.");
                }
                scopes.insert_top(ident, None);
            }
            Ok((states, scopes))
        }
        Stmt::SetBlock(sb) => {
            reject_style_control_flow(&states)?;
            reject_icons_in_set_block(&sb.body)?;
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            let target_idents = extract_bound_idents(&sb.target)?;
            if target_idents
                .iter()
                .any(|id| matches!(id.as_str(), "entry" | "term" | "field"))
            {
                return Err("C-SEC cấm gán lại biến ngữ cảnh qua set block.");
            }
            let (sb_states, sb_scopes) = walk_theme_stmts_with_url_scopes(
                &sb.body,
                BTreeSet::from([ThemeHtmlState::with_depths(
                    ThemeHtmlState::Body,
                    allow_trusted_theme_markup || allow_search_form,
                    HtmlDepths::default(),
                )]),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                is_taxonomy_template,
                policy,
                allow_trusted_theme_markup,
                allow_search_form,
                usage,
            )?;
            if sb_states
                .iter()
                .any(|s| s.inner() != &ThemeHtmlState::Body || s.has_open_depths())
            {
                return Err("Khối set block phải đóng đủ thẻ/comment HTML bên trong.");
            }
            scopes = UrlScopeStack::intersect(&scopes, &sb_scopes);
            for ident in target_idents {
                scopes.insert_top(ident, None);
            }
            Ok((states, scopes))
        }
        Stmt::ForLoop(f) => {
            reject_style_control_flow(&states)?;
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            record_usage_flags_in_expr(&f.iter, usage);
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
                record_usage_flags_in_expr(cond, usage);
            }

            let (else_states, else_scopes) = walk_theme_stmts_with_url_scopes(
                &f.else_body,
                states.clone(),
                scopes.clone(),
                is_part,
                is_full_doctype_doc,
                is_taxonomy_template,
                policy,
                allow_trusted_theme_markup,
                allow_search_form,
                usage,
            )?;

            let target_idents = extract_bound_idents(&f.target)?;
            let iter_kind = resolve_collection_source(&f.iter, &scopes, policy);
            if target_idents.iter().any(|id| id == "entry") {
                let valid_entry_loop = matches!(&f.target, Expr::Var(v) if v.id == "entry")
                    && matches!(
                        iter_kind,
                        Some(
                            UrlSourceKind::TrustedEntry
                                | UrlSourceKind::FieldNode(FieldTemplateNode::EntryRef { .. })
                        )
                    );
                if !valid_entry_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `entry` từ nguồn không tin cậy.");
                }
            }
            if target_idents.iter().any(|id| id == "field") {
                let valid_field_loop = matches!(&f.target, Expr::Var(v) if v.id == "field")
                    && iter_kind == Some(UrlSourceKind::TrustedField);
                if !valid_field_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `field` từ nguồn không tin cậy.");
                }
            }
            if target_idents.iter().any(|id| id == "term") {
                if is_taxonomy_template {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `term` trong template taxonomy.");
                }
                let valid_term_loop = matches!(&f.target, Expr::Var(v) if v.id == "term")
                    && matches!(
                        iter_kind,
                        Some(
                            UrlSourceKind::TrustedLink
                                | UrlSourceKind::FieldNode(FieldTemplateNode::TermRef { .. })
                        )
                    );
                if !valid_term_loop {
                    return Err("C-SEC cấm gán lại biến ngữ cảnh `term` từ nguồn không tin cậy.");
                }
            }

            let loop_targets: BTreeMap<String, Option<UrlSourceKind>> =
                if let Expr::Var(v) = &f.target {
                    BTreeMap::from([(v.id.to_owned(), iter_kind)])
                } else {
                    target_idents.into_iter().map(|id| (id, None)).collect()
                };

            let mut loop_states = states;
            let mut loop_scopes = scopes;
            loop_scopes.push_frame(loop_targets.clone());

            let mut settled = false;
            let mut final_loop_states = BTreeSet::new();
            let mut final_loop_scopes = loop_scopes.clone();

            for _ in 0..MAX_THEME_WALKER_STATES {
                let (after_states, after_scopes) = walk_theme_stmts_with_url_scopes(
                    &f.body,
                    loop_states.clone(),
                    loop_scopes.clone(),
                    is_part,
                    is_full_doctype_doc,
                    is_taxonomy_template,
                    policy,
                    allow_trusted_theme_markup,
                    allow_search_form,
                    usage,
                )?;
                let mut next_states = loop_states.clone();
                next_states.extend(after_states);
                if next_states.len() > MAX_THEME_WALKER_STATES {
                    return Err("Quá nhiều trạng thái HTML trong vòng lặp.");
                }
                let next_scopes = UrlScopeStack::intersect(&loop_scopes, &after_scopes);
                let mut next_header_scopes = next_scopes.clone();
                for (k, v) in &loop_targets {
                    next_header_scopes.insert_top(k.clone(), v.clone());
                }
                if next_states == loop_states && next_header_scopes == loop_scopes {
                    final_loop_states = next_states;
                    final_loop_scopes = next_scopes;
                    settled = true;
                    break;
                }
                loop_states = next_states;
                loop_scopes = next_header_scopes;
            }
            if !settled {
                return Err("Không thể hội tụ trạng thái HTML/URL trong vòng lặp.");
            }

            final_loop_scopes.pop_frame();
            let mut combined_states = final_loop_states;
            combined_states.extend(else_states);
            if combined_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML sau vòng lặp.");
            }
            let combined_scopes = UrlScopeStack::intersect(&final_loop_scopes, &else_scopes);
            Ok((combined_states, combined_scopes))
        }
        _ => Err("Template sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}
