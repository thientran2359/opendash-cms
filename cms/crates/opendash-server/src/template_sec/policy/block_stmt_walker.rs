use super::*;

pub(crate) fn walk_block_ast_stmt(
    stmt: &Stmt<'_>,
    states: BTreeSet<ThemeHtmlState>,
    mut scopes: UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> Result<(BTreeSet<ThemeHtmlState>, UrlScopeStack), &'static str> {
    match stmt {
        Stmt::Template(t) => walk_block_ast_sequence(&t.children, states, scopes, policy),
        Stmt::EmitRaw(raw) => {
            let mut next_states = BTreeSet::new();
            for initial in states {
                let mut cur = initial;
                for ch in raw.raw.chars() {
                    let depth = cur.trusted_depth();
                    cur = step_theme_raw_char(cur, ch, BLOCK_HTML_STEP_OPTIONS, depth)?;
                }
                next_states.insert(cur);
            }
            if next_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML trong template block.");
            }
            Ok((next_states, scopes))
        }
        Stmt::EmitExpr(e) => {
            reject_style_control_flow(&states)?;
            if let Some((icon_arg, _)) = parse_od_icon_call_ext(&e.expr, true)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_icon() chỉ được xuất trong thân HTML của block.");
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
            if let Some(img_call) = parse_od_image_call(&e.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_image() chỉ được xuất trong thân HTML của block.");
                }
                if !is_valid_od_image_media_target(
                    img_call.media_root,
                    &img_call.media_attrs,
                    &scopes,
                    policy,
                ) {
                    return Err("od_image() động phải tham chiếu trường kiểu media.");
                }
                if let Some(alt_expr) = img_call.alt_expr {
                    check_block_expr_restrictions(alt_expr, &scopes, policy)?;
                    if block_expr_touches_sensitive(alt_expr, &scopes, policy) {
                        return Err(
                            "od_image() alt động chỉ được tham chiếu trường văn bản an toàn.",
                        );
                    }
                }
                return Ok((states, scopes));
            }
            if let Some((_, arg_expr)) = parse_od_hook_call(&e.expr)? {
                if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                    return Err("od_hook() chỉ được xuất trong thân HTML của block.");
                }
                if let Some(arg) = arg_expr {
                    check_block_expr_restrictions(arg, &scopes, policy)?;
                }
                return Ok((states, scopes));
            }
            check_block_expr_restrictions(&e.expr, &scopes, policy)?;
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
                    "C-SEC cấm nội suy Jinja ở comment, <style>/<script> hoặc biên thẻ HTML.",
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
                match classify_block_sensitive_expr(&e.expr, &scopes, policy) {
                    Some(BlockSensitiveKind::Richtext) => {
                        if states.iter().any(|s| s.inner() != &ThemeHtmlState::Body) {
                            return Err(
                                "Biến richtext chỉ được phép đặt trong phần thân HTML (ThemeHtmlState::Body).",
                            );
                        }
                    }
                    Some(BlockSensitiveKind::HrefUrl | BlockSensitiveKind::MediaUrl) => {
                        return Err(
                            "Biến url / media url chỉ được xuất bên trong thuộc tính URL được trích dẫn.",
                        );
                    }
                    Some(BlockSensitiveKind::ObjectOrNonScalar) => {
                        return Err(
                            "Không được xuất trực tiếp đối tượng media/link/icon/group/repeater/options/menus.",
                        );
                    }
                    None => {
                        if block_expr_touches_sensitive(&e.expr, &scopes, policy) {
                            return Err(
                                "Biến richtext/url/media không được phép ghép chuỗi hoặc biến đổi trong biểu thức xuất.",
                            );
                        }
                    }
                }
                return Ok((states, scopes));
            }

            if matches!(
                classify_block_sensitive_expr(&e.expr, &scopes, policy),
                Some(BlockSensitiveKind::Richtext | BlockSensitiveKind::ObjectOrNonScalar)
            ) {
                return Err(
                    "C-SEC cấm xuất biến richtext hoặc đối tượng không phải chuỗi bên trong thuộc tính thẻ HTML.",
                );
            }

            let mut expected_url_slot: Option<Option<UrlAttrSlot>> = None;
            let mut updated_states = BTreeSet::new();
            for state in states {
                let svg_depth = state.trusted_depth();
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
                if forbids_dynamic_interpolation(&attr_name)
                    || matches!(attr_name.as_str(), "style" | "srcset" | "ping")
                    || (tag_name == "link" && attr_name == "rel")
                    || tag_name == "meta"
                {
                    return Err(
                        "C-SEC cấm nội suy bên trong thuộc tính bảo mật hoặc sự kiện HTML.",
                    );
                }
                if matches!(attr_name.as_str(), "href" | "src" | "poster" | "cite") {
                    if attr_has_static_bytes || attr_expr_count != 0 {
                        return Err(
                            "URL động trong thuộc tính chỉ được phép là biểu thức {{ ... }} đơn.",
                        );
                    }
                    let Some(slot) = classify_theme_url_slot(&tag_name, &attr_name) else {
                        return Err("C-SEC cấm nội suy URL động trên thuộc tính hoặc thẻ này.");
                    };
                    if let Some(prev_slot) = expected_url_slot {
                        if prev_slot != Some(slot) {
                            return Err("Nhóm thuộc tính URL phải đồng nhất qua các nhánh rẽ.");
                        }
                    } else {
                        expected_url_slot = Some(Some(slot));
                    }
                    if !is_valid_block_url_expr(&e.expr, slot, &scopes, policy) {
                        return Err(
                            "URL động trong thuộc tính không nằm trong whitelist typed URL của block.",
                        );
                    }
                    attr_expr_count = 1;
                    if slot == UrlAttrSlot::LinkCss {
                        link_has_valid_href = true;
                    }
                } else {
                    if block_expr_touches_sensitive(&e.expr, &scopes, policy) {
                        return Err(
                            "Biến richtext/url/media không được phép xuất bên trong thuộc tính HTML thường.",
                        );
                    }
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
                updated_states.insert(ThemeHtmlState::with_trust(
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
                        attr_static_buf,
                    },
                    true,
                    svg_depth,
                ));
            }
            Ok((updated_states, scopes))
        }
        Stmt::IfCond(cond) => {
            reject_style_control_flow(&states)?;
            check_block_expr_restrictions(&cond.expr, &scopes, policy)?;
            let (mut true_states, true_scopes) =
                walk_block_ast_sequence(&cond.true_body, states.clone(), scopes.clone(), policy)?;
            let (false_states, false_scopes) =
                walk_block_ast_sequence(&cond.false_body, states, scopes, policy)?;
            true_states.extend(false_states);
            if true_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML tại nhánh điều kiện block.");
            }
            Ok((
                true_states,
                UrlScopeStack::intersect(&true_scopes, &false_scopes),
            ))
        }
        Stmt::ForLoop(loop_stmt) => {
            reject_style_control_flow(&states)?;
            validate_expr(&loop_stmt.target)?;
            check_block_expr_restrictions(&loop_stmt.iter, &scopes, policy)?;
            if let Some(cond) = &loop_stmt.filter_expr {
                check_block_expr_restrictions(cond, &scopes, policy)?;
            }

            let target_idents = extract_bound_idents(&loop_stmt.target)?;
            for id in &target_idents {
                if is_protected_block_binding(id, &scopes) {
                    return Err("Biến vòng lặp for không được che khuất tên biến nhạy cảm.");
                }
            }

            let iter_kind = resolve_block_collection_source(&loop_stmt.iter, &scopes, policy);
            if block_expr_touches_sensitive(&loop_stmt.iter, &scopes, policy) {
                let valid_iter = matches!(&loop_stmt.target, Expr::Var(_)) && iter_kind.is_some();
                if !valid_iter {
                    return Err(
                        "Không được lặp trực tiếp trên biến richtext/url/media hoặc sai cú pháp vòng lặp block.",
                    );
                }
            }

            let (else_states, else_scopes) = walk_block_ast_sequence(
                &loop_stmt.else_body,
                states.clone(),
                scopes.clone(),
                policy,
            )?;

            let loop_targets: BTreeMap<String, Option<UrlSourceKind>> =
                if let Expr::Var(v) = &loop_stmt.target {
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
                let (after_states, after_scopes) = walk_block_ast_sequence(
                    &loop_stmt.body,
                    loop_states.clone(),
                    loop_scopes.clone(),
                    policy,
                )?;
                let mut next_states = loop_states.clone();
                next_states.extend(after_states);
                if next_states.len() > MAX_THEME_WALKER_STATES {
                    return Err("Quá nhiều trạng thái HTML trong vòng lặp block.");
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
                return Err("Không thể xác định vị trí HTML sau vòng lặp trong template block.");
            }

            final_loop_scopes.pop_frame();
            let mut combined_states = final_loop_states;
            combined_states.extend(else_states);
            if combined_states.len() > MAX_THEME_WALKER_STATES {
                return Err("Quá nhiều trạng thái HTML sau vòng lặp block.");
            }
            let combined_scopes = UrlScopeStack::intersect(&final_loop_scopes, &else_scopes);
            Ok((combined_states, combined_scopes))
        }
        Stmt::Set(s) => {
            reject_style_control_flow(&states)?;
            validate_expr(&s.target)?;
            check_block_expr_restrictions(&s.expr, &scopes, policy)?;
            if block_expr_touches_sensitive(&s.expr, &scopes, policy) {
                return Err(
                    "C-SEC cấm gán (set) hoặc tạo bí danh cho biến richtext/url/media/repeater.",
                );
            }
            for ident in extract_bound_idents(&s.target)? {
                if is_protected_block_binding(&ident, &scopes) {
                    return Err("C-SEC cấm gán (set) hoặc ghi đè biến nhạy cảm của block.");
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
                .any(|id| is_protected_block_binding(id, &scopes))
            {
                return Err("C-SEC cấm ghi đè biến nhạy cảm qua khối set.");
            }
            for child in &sb.body {
                if block_stmt_touches_sensitive(child, &scopes, policy) {
                    return Err("C-SEC cấm bọc biến richtext/url/media trong khối set.");
                }
            }
            let (sb_states, sb_scopes) = walk_block_ast_sequence(
                &sb.body,
                BTreeSet::from([ThemeHtmlState::with_trust(ThemeHtmlState::Body, true, 0)]),
                scopes.clone(),
                policy,
            )?;
            if sb_states
                .iter()
                .any(|s| s.inner() != &ThemeHtmlState::Body || s.trusted_depth() != 0)
            {
                return Err("Khối set block phải đóng đủ thẻ/comment HTML bên trong.");
            }
            scopes = UrlScopeStack::intersect(&scopes, &sb_scopes);
            for ident in target_idents {
                scopes.insert_top(ident, None);
            }
            Ok((states, scopes))
        }
        Stmt::WithBlock(w) => {
            reject_style_control_flow(&states)?;
            let mut frame = BTreeMap::new();
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                check_block_expr_restrictions(v, &scopes, policy)?;
                if block_expr_touches_sensitive(v, &scopes, policy) {
                    return Err(
                        "C-SEC cấm tạo bí danh cho biến richtext/url/media/repeater qua khối with.",
                    );
                }
                for ident in extract_bound_idents(k)? {
                    if is_protected_block_binding(&ident, &scopes) {
                        return Err("C-SEC cấm ghi đè biến nhạy cảm qua khối with.");
                    }
                    frame.insert(ident, None);
                }
            }
            scopes.push_frame(frame);
            let (out_states, mut out_scopes) =
                walk_block_ast_sequence(&w.body, states, scopes, policy)?;
            out_scopes.pop_frame();
            Ok((out_states, out_scopes))
        }
        _ => Err("Template block sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

pub(crate) fn block_stmt_touches_sensitive(
    stmt: &Stmt<'_>,
    scopes: &UrlScopeStack,
    policy: &ThemeUrlPolicy,
) -> bool {
    match stmt {
        Stmt::Template(t) => t
            .children
            .iter()
            .any(|c| block_stmt_touches_sensitive(c, scopes, policy)),
        Stmt::EmitRaw(_) => false,
        Stmt::EmitExpr(e) => block_expr_touches_sensitive(&e.expr, scopes, policy),
        Stmt::IfCond(i) => {
            block_expr_touches_sensitive(&i.expr, scopes, policy)
                || i.true_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
                || i.false_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::ForLoop(f) => {
            block_expr_touches_sensitive(&f.iter, scopes, policy)
                || f.body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
                || f.else_body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::Set(s) => {
            block_expr_touches_sensitive(&s.target, scopes, policy)
                || block_expr_touches_sensitive(&s.expr, scopes, policy)
        }
        Stmt::SetBlock(sb) => {
            block_expr_touches_sensitive(&sb.target, scopes, policy)
                || sb
                    .body
                    .iter()
                    .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        Stmt::WithBlock(w) => {
            w.assignments.iter().any(|(k, v)| {
                block_expr_touches_sensitive(k, scopes, policy)
                    || block_expr_touches_sensitive(v, scopes, policy)
            }) || w
                .body
                .iter()
                .any(|c| block_stmt_touches_sensitive(c, scopes, policy))
        }
        _ => true,
    }
}

/// Validates a declarative `block.json` v1 `render.html` template using the unified
/// `ThemeHtmlState` + MiniJinja AST + `UrlScopeStack` checker.
pub fn validate_block_template_csec(
    block_id: &str,
    source: &str,
    schema: &opendash_core::block_schema::BlockSchemaV1,
) -> Result<(), &'static str> {
    if source.trim().is_empty() {
        return Err("Template block không được để trống.");
    }
    if source.len() > MAX_RENDERED_HTML_BYTES {
        return Err("Template block vượt giới hạn 512 KiB.");
    }
    if source.contains(BLOCK_RICHTEXT_NONCE_PREFIX)
        || source.contains(CONTENT_SLOT_TOKEN)
        || source.contains(PREVIEW_PART_NONCE_PREFIX)
        || source.contains(ICON_NONCE_PREFIX)
        || source.contains(IMAGE_NONCE_PREFIX)
        || source.contains(HOOK_NONCE_PREFIX)
    {
        return Err("Template block chứa chuỗi nonce nội bộ bị cấm.");
    }

    let ctx = BlockSchemaTemplateContext::from_schema(schema);
    let ast = parse(source, block_id, SyntaxConfig::default())
        .map_err(|_| "Cú pháp MiniJinja của template block không hợp lệ.")?;
    validate_stmt_with_icons_ext(&ast, true, true, false)?;

    let (final_states, _) = walk_block_ast_stmt(
        &ast,
        BTreeSet::from([ThemeHtmlState::with_trust(ThemeHtmlState::Body, true, 0)]),
        ctx.root_scopes,
        &ctx.policy,
    )?;
    if final_states
        .iter()
        .any(|s| s.inner() != &ThemeHtmlState::Body || s.trusted_depth() != 0)
    {
        return Err(
            "Template block phải kết thúc ở trạng thái thân HTML (đóng đủ thẻ/comment/svg).",
        );
    }

    let mut env = restricted_environment();
    env.add_template(block_id, source)
        .map_err(|_| "Không thể biên dịch template block trong môi trường C-SEC.")?;
    Ok(())
}
