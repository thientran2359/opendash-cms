use super::*;

pub(super) fn push_box_spacing_css(css: &mut String, prop: &str, spacing: Option<BoxSpacing>) {
    let Some(s) = spacing else { return };
    if let Some(v) = s.top {
        css.push_str(&format!("{prop}-top:{v}px;"));
    }
    if let Some(v) = s.right {
        css.push_str(&format!("{prop}-right:{v}px;"));
    }
    if let Some(v) = s.bottom {
        css.push_str(&format!("{prop}-bottom:{v}px;"));
    }
    if let Some(v) = s.left {
        css.push_str(&format!("{prop}-left:{v}px;"));
    }
}

pub(super) fn plan_container_styles(
    responsive: Option<&Responsive>,
    kind: ContainerKind,
    row_section_width: Option<SectionWidth>,
) -> ContainerRenderPlan {
    let mut tailwind_classes = Vec::new();
    let mut fallback_desktop_css = String::new();
    let mut fallback_tablet_css = String::new();
    let mut fallback_mobile_css = String::new();

    if matches!(kind, ContainerKind::Row)
        && let Some(cw) = row_section_width
    {
        match cw {
            SectionWidth::Boxed => {
                tailwind_classes.extend(["w-full", "max-w-7xl", "mx-auto"].map(str::to_owned));
            }
            SectionWidth::Wide => {
                tailwind_classes
                    .extend(["w-full", "max-w-screen-2xl", "mx-auto"].map(str::to_owned));
            }
            SectionWidth::Full => {
                tailwind_classes.extend(["w-full", "max-w-none"].map(str::to_owned));
            }
        }
    }

    let Some(resp) = responsive else {
        return ContainerRenderPlan {
            tailwind_classes,
            fallback_desktop_css,
            fallback_tablet_css,
            fallback_mobile_css,
        };
    };

    let d = resp.desktop.as_ref();
    let t = resp.tablet.as_ref();
    let m = resp.mobile.as_ref();

    let (d_pad, t_pad, m_pad) = (
        box_or_legacy_px(d),
        box_or_legacy_px(t),
        box_or_legacy_px(m),
    );
    if let Some(pad_classes) = compile_box_spacing_tailwind("p", d_pad, t_pad, m_pad, false) {
        tailwind_classes.extend(pad_classes);
    } else {
        push_box_spacing_css(&mut fallback_desktop_css, "padding", d_pad);
        push_box_spacing_css(&mut fallback_tablet_css, "padding", t_pad);
        push_box_spacing_css(&mut fallback_mobile_css, "padding", m_pad);
    }

    let (d_mar, t_mar, m_mar) = (
        d.and_then(|s| s.margin).filter(|b| !b.is_empty()),
        t.and_then(|s| s.margin).filter(|b| !b.is_empty()),
        m.and_then(|s| s.margin).filter(|b| !b.is_empty()),
    );
    if let Some(mar_classes) = compile_box_spacing_tailwind("m", d_mar, t_mar, m_mar, true) {
        tailwind_classes.extend(mar_classes);
    } else {
        push_box_spacing_css(&mut fallback_desktop_css, "margin", d_mar);
        push_box_spacing_css(&mut fallback_tablet_css, "margin", t_mar);
        push_box_spacing_css(&mut fallback_mobile_css, "margin", m_mar);
    }

    if matches!(kind, ContainerKind::Row) && resp.opts_into_grid() {
        tailwind_classes.push("grid".to_owned());
        let d_col = d.and_then(|s| s.columns_per_row);
        let t_col = t.and_then(|s| s.columns_per_row).or(d_col);
        let m_col = m.and_then(|s| s.columns_per_row).or(t_col);
        let m_v = m_col.unwrap_or(1);
        let t_v = t_col.unwrap_or(1);
        let d_v = d_col.unwrap_or(1);
        tailwind_classes.push(format!("grid-cols-{m_v}"));
        if t_v != m_v {
            tailwind_classes.push(format!("md:grid-cols-{t_v}"));
        }
        if d_v != t_v {
            tailwind_classes.push(format!("lg:grid-cols-{d_v}"));
        }

        let d_gap = d.and_then(|s| s.gap_px);
        let t_gap = t.and_then(|s| s.gap_px).or(d_gap);
        let m_gap = m.and_then(|s| s.gap_px).or(t_gap);
        if d_gap.is_some() || t_gap.is_some() || m_gap.is_some() {
            let m_g = m_gap.unwrap_or(0);
            let t_g = t_gap.unwrap_or(0);
            let d_g = d_gap.unwrap_or(0);
            if let (Some(gm), Some(gt), Some(gd)) = (
                gap_px_to_tw_step(m_g),
                gap_px_to_tw_step(t_g),
                gap_px_to_tw_step(d_g),
            ) {
                tailwind_classes.push(format!("gap-{gm}"));
                if gt != gm {
                    tailwind_classes.push(format!("md:gap-{gt}"));
                }
                if gd != gt {
                    tailwind_classes.push(format!("lg:gap-{gd}"));
                }
            } else {
                fallback_desktop_css.push_str(&format!("gap:{}px;", d_gap.unwrap_or(0)));
                if let Some(tg) = t.and_then(|s| s.gap_px) {
                    fallback_tablet_css.push_str(&format!("gap:{tg}px;"));
                }
                if let Some(mg) = m.and_then(|s| s.gap_px) {
                    fallback_mobile_css.push_str(&format!("gap:{mg}px;"));
                }
            }
        }
    }

    if matches!(kind, ContainerKind::Column) {
        let d_al = d.and_then(|s| s.text_align);
        let t_al = t.and_then(|s| s.text_align).or(d_al);
        let m_al = m.and_then(|s| s.text_align).or(t_al);
        if d_al.is_some() || t_al.is_some() || m_al.is_some() {
            let m_cls = align_tailwind_class(m_al.unwrap_or(TextAlign::Start));
            let t_cls = align_tailwind_class(t_al.unwrap_or(TextAlign::Start));
            let d_cls = align_tailwind_class(d_al.unwrap_or(TextAlign::Start));
            tailwind_classes.push(m_cls.to_owned());
            if t_cls != m_cls {
                tailwind_classes.push(format!("md:{t_cls}"));
            }
            if d_cls != t_cls {
                tailwind_classes.push(format!("lg:{d_cls}"));
            }
        }
    }

    ContainerRenderPlan {
        tailwind_classes,
        fallback_desktop_css,
        fallback_tablet_css,
        fallback_mobile_css,
    }
}

/// Emits `<style data-builder-responsive>` only for properties that cannot be represented
/// as static Tailwind utility classes: Section backgrounds, node Visibility, and custom pixel fallbacks.
pub(super) fn builder_responsive_css(
    document: &BuilderDocument,
    is_preview: bool,
    hydration_media: &std::collections::BTreeMap<String, crate::storage::fields::HydratedMediaRow>,
) -> String {
    let mut desktop = String::new();
    let mut tablet = String::new();
    let mut mobile = String::new();
    let mut vis_desktop = String::new();
    let mut vis_tablet = String::new();
    let mut vis_mobile = String::new();
    let mut append_visibility = |id: &str, meta: Option<&NodeMeta>| {
        let Some(vis) = meta.and_then(|m| m.visibility.as_ref()) else {
            return;
        };
        if vis.hide_desktop {
            push_visibility_rule(&mut vis_desktop, id);
        }
        if vis.hide_tablet {
            push_visibility_rule(&mut vis_tablet, id);
        }
        if vis.hide_mobile {
            push_visibility_rule(&mut vis_mobile, id);
        }
    };
    for section in &document.sections {
        if !section.is_synthetic() {
            let plan =
                plan_container_styles(section.responsive.as_ref(), ContainerKind::Section, None);
            push_scoped_rule(&mut desktop, &section.id, &plan.fallback_desktop_css);
            push_scoped_rule(&mut tablet, &section.id, &plan.fallback_tablet_css);
            push_scoped_rule(&mut mobile, &section.id, &plan.fallback_mobile_css);
            append_visibility(&section.id, section.meta.as_ref());
            if let Some(style) = section.style.as_ref()
                && valid_package_id(&section.id)
            {
                let mut sec_css = String::new();
                if let Some(bg_color) = style
                    .background_color
                    .as_deref()
                    .filter(|s| !s.is_empty() && opendash_core::field_schema::valid_color_value(s))
                {
                    sec_css.push_str(&format!("background-color:{bg_color};"));
                }
                if let Some(bg_media) = style
                    .background_media_id
                    .as_deref()
                    .filter(|id| !id.is_empty() && valid_package_id(id))
                {
                    let bg_url = super::media_render::versioned_media_file_url(
                        bg_media,
                        hydration_media,
                        is_preview,
                    );
                    sec_css.push_str(&format!(
                        "background-image:url(\"{bg_url}\");background-size:cover;background-position:center;"
                    ));
                }
                push_scoped_rule(&mut desktop, &section.id, &sec_css);
            }
        }
        let row_section_width = (!section.is_synthetic())
            .then(|| section.style.as_ref().and_then(|s| s.content_width))
            .flatten();
        for row in &section.children {
            if !row.is_synthetic() {
                let plan = plan_container_styles(
                    row.responsive.as_ref(),
                    ContainerKind::Row,
                    row_section_width,
                );
                push_scoped_rule(&mut desktop, &row.id, &plan.fallback_desktop_css);
                push_scoped_rule(&mut tablet, &row.id, &plan.fallback_tablet_css);
                push_scoped_rule(&mut mobile, &row.id, &plan.fallback_mobile_css);
                append_visibility(&row.id, row.meta.as_ref());
            }
            for column in &row.children {
                if !column.is_synthetic() {
                    let plan = plan_container_styles(
                        column.responsive.as_ref(),
                        ContainerKind::Column,
                        None,
                    );
                    push_scoped_rule(&mut desktop, &column.id, &plan.fallback_desktop_css);
                    push_scoped_rule(&mut tablet, &column.id, &plan.fallback_tablet_css);
                    push_scoped_rule(&mut mobile, &column.id, &plan.fallback_mobile_css);
                    append_visibility(&column.id, column.meta.as_ref());
                }
                for element in &column.children {
                    append_visibility(&element.id, element.meta.as_ref());
                }
            }
        }
    }
    if desktop.is_empty()
        && tablet.is_empty()
        && mobile.is_empty()
        && vis_desktop.is_empty()
        && vis_tablet.is_empty()
        && vis_mobile.is_empty()
    {
        return String::new();
    }
    let mut css = format!("<style data-builder-responsive>{desktop}");
    if !tablet.is_empty() {
        css.push_str(&format!("@media (width < 1024px){{{tablet}}}"));
    }
    if !mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{mobile}}}"));
    }
    if !vis_desktop.is_empty() {
        css.push_str(&format!("@media (width >= 1024px){{{vis_desktop}}}"));
    }
    if !vis_tablet.is_empty() {
        css.push_str(&format!(
            "@media (width >= 768px) and (width < 1024px){{{vis_tablet}}}"
        ));
    }
    if !vis_mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{vis_mobile}}}"));
    }
    css.push_str("</style>");
    css
}

pub(super) fn format_css_number(
    n: f64,
    unit: Option<opendash_core::field_schema::CssUnit>,
) -> Option<String> {
    if !n.is_finite() {
        return None;
    }
    let suffix = unit.map(|u| u.as_css_suffix()).unwrap_or("");
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Some(format!("{}{suffix}", n as i64))
    } else {
        Some(format!("{n}{suffix}"))
    }
}
