use std::collections::BTreeMap;

use opendash_core::{
    builder::BlockNode,
    field_schema::valid_safe_url,
};
use serde_json::Value;

use super::{
    blocks_layout::{responsive_str_cascade, str_prop},
    render::escape_html,
};
use crate::{
    content_render::{CoreL10n, TemplateRenderData, core_l10n},
    error::{ApiError, ApiResult},
    storage::menu::MenuItemView,
    template_sec::{RenderBudget, register_core_icon_in_budget},
};

pub(super) fn preview_empty_placeholder(label: &str, is_preview: bool) -> Option<String> {
    is_preview.then(|| {
        format!(
            "<div class=\"od-tpl-placeholder rounded border border-dashed border-od-border px-3 py-2 text-xs text-od-muted\">{}</div>",
            escape_html(label),
        )
    })
}

pub(super) fn align_text_classes(props: &BTreeMap<String, Value>, default: &str) -> Vec<String> {
    let (is_resp, d, t, m) = responsive_str_cascade(props, "align", default);
    let map_cls = |v: &str| match v {
        "center" => "text-center",
        "right" => "text-right",
        _ => "text-left",
    };
    let (mc, tc, dc) = (map_cls(m), map_cls(t), map_cls(d));
    let mut cls = vec![mc.to_owned()];
    if is_resp && tc != mc {
        cls.push(format!("md:{tc}"));
    }
    if is_resp && dc != tc {
        cls.push(format!("lg:{dc}"));
    }
    cls
}

pub(super) fn align_justify_classes(props: &BTreeMap<String, Value>, default: &str) -> Vec<String> {
    let (is_resp, d, t, m) = responsive_str_cascade(props, "align", default);
    let map_cls = |v: &str| match v {
        "center" => "justify-center",
        "right" => "justify-end",
        _ => "justify-start",
    };
    let (mc, tc, dc) = (map_cls(m), map_cls(t), map_cls(d));
    let mut cls = vec![mc.to_owned()];
    if is_resp && tc != mc {
        cls.push(format!("md:{tc}"));
    }
    if is_resp && dc != tc {
        cls.push(format!("lg:{dc}"));
    }
    cls
}

pub(super) fn bool_prop(props: &BTreeMap<String, Value>, key: &str, default: bool) -> bool {
    props.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn render_site_brand(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
) -> Option<String> {
    let Some(td) = td else {
        return preview_empty_placeholder("Site Brand", is_preview);
    };
    let mode = str_prop(&node.props, "display_mode", "logo_and_title");
    let tag = match str_prop(&node.props, "title_tag", "span") {
        s @ ("span" | "p" | "h1") => s,
        _ => "span",
    };
    let link_home = bool_prop(&node.props, "link_home", true);
    let mut cls = vec![
        "od-site-brand".to_owned(),
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-2.5".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "left"));

    let has_color = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let color_style = if has_color {
        " style=\"color:var(--od-brand-color);\""
    } else {
        ""
    };

    let mut logo_html = String::new();
    if matches!(mode, "logo_only" | "logo_and_title")
        && let Some(logo) = td.site_logo
    {
        let input = logo.as_picture_input(
            "200px",
            "od-brand-logo-img w-auto object-contain",
            Some(td.site.title),
            "eager",
            "",
        );
        if let Some(pic) = opendash_core::media::render_responsive_picture_html(&input, is_preview)
        {
            logo_html = format!("<span class=\"od-brand-logo inline-flex items-center\">{pic}</span>");
        }
    }

    let show_title = matches!(mode, "title_only" | "logo_and_title" | "title_and_tagline")
        || (mode == "logo_only" && logo_html.is_empty());
    let title_html = if show_title {
        format!(
            "<{tag} class=\"od-brand-title font-bold tracking-tight\">{}</{tag}>",
            escape_html(td.site.title)
        )
    } else {
        String::new()
    };
    let tagline_html = if mode == "title_and_tagline" && !td.site.tagline.trim().is_empty() {
        format!(
            "<span class=\"od-brand-tagline text-xs text-od-muted block\">{}</span>",
            escape_html(td.site.tagline)
        )
    } else {
        String::new()
    };

    let inner = if tagline_html.is_empty() {
        format!("{logo_html}{title_html}")
    } else {
        format!("{logo_html}<span class=\"flex flex-col\">{title_html}{tagline_html}</span>")
    };
    let body = if link_home && valid_safe_url(&td.locale.home_url) {
        format!(
            "<a href=\"{}\" class=\"inline-flex items-center gap-2.5 text-inherit no-underline\">{inner}</a>",
            escape_html(&td.locale.home_url),
        )
    } else {
        inner
    };

    Some(format!(
        "<div class=\"{}\"{color_style}>{body}</div>",
        cls.join(" ")
    ))
}

fn render_language_switcher(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    locale: &str,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let l10n = core_l10n(locale);
    let style = str_prop(&node.props, "style", "dropdown");
    let label_mode = str_prop(&node.props, "label_mode", "native_name");

    let mut cls = vec![
        "od-lang-switcher".to_owned(),
        "flex".to_owned(),
        "items-center".to_owned(),
    ];
    cls.extend(align_justify_classes(&node.props, "right"));

    let alternates = td.map(|d| d.locale.alternates.as_slice()).unwrap_or(&[]);
    if alternates.len() <= 1 {
        if !is_preview {
            return Ok(None);
        }
        let stub = if label_mode == "code" {
            "[VI / EN]"
        } else {
            "[Tiếng Việt / English]"
        };
        return Ok(Some(format!(
            "<nav class=\"{}\" aria-label=\"{}\"><span class=\"inline-flex items-center gap-1.5 rounded border border-dashed border-od-border px-2.5 py-1 text-xs text-od-muted\">{stub}</span></nav>",
            cls.join(" "),
            escape_html(l10n.language_switcher.as_ref()),
        )));
    }

    let label_for = |code: &str, label: &str| -> String {
        if label_mode == "code" {
            code.to_ascii_uppercase()
        } else {
            label.to_owned()
        }
    };

    let current_alt = alternates
        .iter()
        .find(|a| a.is_current)
        .or_else(|| alternates.first());

    let inner = match style {
        "dropdown" => {
            let cur_text = current_alt
                .map(|a| label_for(&a.code, &a.label))
                .unwrap_or_else(|| locale.to_ascii_uppercase());
            let chev = register_core_icon_in_budget(budget, "chevron-down", Some("w-3.5 h-3.5"))?
                .unwrap_or_default();
            let mut list = String::new();
            for alt in alternates {
                if !valid_safe_url(&alt.url) {
                    continue;
                }
                let text = label_for(&alt.code, &alt.label);
                let cur_attr = if alt.is_current { " aria-current=\"true\" class=\"block px-3 py-1.5 text-xs font-semibold text-od-primary\"" } else { " class=\"block px-3 py-1.5 text-xs hover:bg-od-surface-alt\"" };
                list.push_str(&format!(
                    "<li><a href=\"{}\"{cur_attr}>{}</a></li>",
                    escape_html(&alt.url),
                    escape_html(&text),
                ));
            }
            format!(
                "<details class=\"relative\"><summary class=\"inline-flex cursor-pointer list-none items-center gap-1.5 rounded-lg border border-od-border px-3 py-1.5 text-xs font-medium\"><span>{}</span>{chev}</summary><ul class=\"absolute right-0 z-30 mt-1 min-w-32 rounded-lg border border-od-border bg-od-surface py-1 shadow-md list-none p-0\">{list}</ul></details>",
                escape_html(&cur_text),
            )
        }
        "pills" => {
            let mut items = String::new();
            for alt in alternates {
                if !valid_safe_url(&alt.url) {
                    continue;
                }
                let text = label_for(&alt.code, &alt.label);
                let item_cls = if alt.is_current {
                    "rounded-full bg-od-primary px-3 py-1 text-xs font-medium text-white"
                } else {
                    "rounded-full border border-od-border px-3 py-1 text-xs font-medium hover:bg-od-surface-alt"
                };
                let cur_attr = if alt.is_current { " aria-current=\"true\"" } else { "" };
                items.push_str(&format!(
                    "<a href=\"{}\" class=\"{item_cls}\"{cur_attr}>{}</a>",
                    escape_html(&alt.url),
                    escape_html(&text),
                ));
            }
            format!("<div class=\"flex flex-wrap items-center gap-1.5\">{items}</div>")
        }
        _ => {
            let mut items = Vec::new();
            for alt in alternates {
                if !valid_safe_url(&alt.url) {
                    continue;
                }
                let text = label_for(&alt.code, &alt.label);
                let item_cls = if alt.is_current {
                    "text-xs font-semibold text-od-primary"
                } else {
                    "text-xs text-od-muted hover:text-od-text"
                };
                let cur_attr = if alt.is_current { " aria-current=\"true\"" } else { "" };
                items.push(format!(
                    "<a href=\"{}\" class=\"{item_cls}\"{cur_attr}>{}</a>",
                    escape_html(&alt.url),
                    escape_html(&text),
                ));
            }
            format!(
                "<div class=\"flex flex-wrap items-center gap-2\">{}</div>",
                items.join("<span class=\"text-od-muted\" aria-hidden=\"true\">/</span>")
            )
        }
    };

    Ok(Some(format!(
        "<nav class=\"{}\" aria-label=\"{}\">{inner}</nav>",
        cls.join(" "),
        escape_html(l10n.language_switcher.as_ref()),
    )))
}

fn render_menu_items_html(
    items: &[MenuItemView],
    max_depth: usize,
    cur_depth: usize,
    is_vertical: bool,
) -> String {
    if cur_depth > max_depth || items.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for item in items {
        let link_cls = if item.is_active {
            "font-semibold text-od-primary"
        } else {
            "hover:text-od-primary"
        };
        let target_attr = if item.is_external {
            " target=\"_blank\" rel=\"noopener noreferrer\""
        } else {
            ""
        };
        let safe_href = if valid_safe_url(&item.url) {
            escape_html(&item.url)
        } else {
            "#".to_owned()
        };
        let anchor = format!(
            "<a href=\"{safe_href}\" class=\"text-sm {link_cls}\"{target_attr}>{}</a>",
            escape_html(&item.label),
        );
        if cur_depth < max_depth && !item.children.is_empty() {
            let sub_html = render_menu_items_html(&item.children, max_depth, cur_depth + 1, true);
            let sub_box_cls = if is_vertical {
                "mt-1 ml-3 flex flex-col gap-1 list-none p-0"
            } else {
                "md:absolute md:left-0 md:z-30 mt-1 min-w-44 rounded-lg border border-od-border bg-od-surface p-2 flex flex-col gap-1.5 shadow-md list-none"
            };
            out.push_str(&format!(
                "<li class=\"relative\"><details class=\"od-menu-sub\"><summary class=\"inline-flex cursor-pointer list-none items-center gap-1\">{anchor}</summary><ul class=\"{sub_box_cls}\">{sub_html}</ul></details></li>"
            ));
        } else {
            out.push_str(&format!("<li>{anchor}</li>"));
        }
    }
    out
}

fn render_menu_block(
    node: &BlockNode,
    scope: &str,
    td: Option<&TemplateRenderData<'_>>,
    l10n: &CoreL10n,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let source_mode = str_prop(&node.props, "source_mode", "slot");
    let menu = td.and_then(|d| {
        if source_mode == "menu" {
            let key = str_prop(&node.props, "menu_key", "");
            (!key.is_empty()).then(|| d.menus_by_key.get(key)).flatten()
        } else {
            let slot = str_prop(&node.props, "slot_key", "");
            if slot.is_empty() {
                None
            } else {
                d.menus_by_slot.get(slot)
            }
        }
    });

    let Some(menu) = menu else {
        return Ok(preview_empty_placeholder("Navigation Menu", is_preview));
    };

    let layout = str_prop(&node.props, "layout", "horizontal");
    let max_depth = usize::try_from(
        node.props
            .get("max_depth")
            .and_then(Value::as_i64)
            .unwrap_or(2)
            .clamp(1, 3),
    )
    .unwrap_or(2);
    let mobile_drawer = bool_prop(&node.props, "mobile_drawer", true);
    let is_vertical = layout == "vertical";

    let mut nav_cls = vec!["od-menu".to_owned(), "relative".to_owned()];
    let justify = align_justify_classes(&node.props, "left");
    let items_html = render_menu_items_html(&menu.items, max_depth, 1, is_vertical);

    if is_vertical {
        let ul_cls = format!(
            "flex flex-col list-none p-0 {}",
            justify.join(" ")
        );
        return Ok(Some(format!(
            "<nav class=\"{}\" aria-label=\"{}\"><ul class=\"{ul_cls}\" style=\"gap:var(--od-menu-gap,16px);\">{items_html}</ul></nav>",
            nav_cls.join(" "),
            escape_html(&menu.title),
        )));
    }

    if mobile_drawer {
        let toggle_id = format!("od-mt-{scope}-{}", node.id);
        let menu_icon =
            register_core_icon_in_budget(budget, "menu", Some("w-5 h-5"))?.unwrap_or_default();
        let ul_cls = format!(
            "hidden peer-checked:flex flex-col md:flex md:flex-row md:items-center list-none p-0 {}",
            justify.join(" "),
        );
        return Ok(Some(format!(
            "<nav class=\"{}\" aria-label=\"{}\"><input type=\"checkbox\" id=\"{}\" class=\"peer sr-only\" aria-hidden=\"true\"><label for=\"{}\" role=\"button\" aria-label=\"{}\" class=\"inline-flex cursor-pointer items-center gap-2 rounded-lg border border-od-border px-3 py-2 text-sm font-medium md:hidden\">{menu_icon}<span>{}</span></label><ul class=\"{ul_cls}\" style=\"gap:var(--od-menu-gap,16px);\">{items_html}</ul></nav>",
            nav_cls.join(" "),
            escape_html(&menu.title),
            escape_html(&toggle_id),
            escape_html(&toggle_id),
            escape_html(l10n.menu_toggle.as_ref()),
            escape_html(l10n.menu_label.as_ref()),
        )));
    }

    nav_cls.push("flex".to_owned());
    let ul_cls = format!(
        "flex flex-row flex-wrap items-center list-none p-0 {}",
        justify.join(" ")
    );
    Ok(Some(format!(
        "<nav class=\"{}\" aria-label=\"{}\"><ul class=\"{ul_cls}\" style=\"gap:var(--od-menu-gap,16px);\">{items_html}</ul></nav>",
        nav_cls.join(" "),
        escape_html(&menu.title),
    )))
}

fn render_breadcrumbs_block(
    node: &BlockNode,
    td: Option<&TemplateRenderData<'_>>,
    l10n: &CoreL10n,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> Result<Option<String>, &'static str> {
    let hide_on_home = bool_prop(&node.props, "hide_on_home", true);
    let crumbs = td.map(|d| d.breadcrumbs).unwrap_or(&[]);
    if crumbs.is_empty() || (hide_on_home && crumbs.len() <= 1) {
        return Ok(preview_empty_placeholder("Breadcrumbs", is_preview));
    }

    let separator = str_prop(&node.props, "separator", "chevron");
    let show_home_icon = bool_prop(&node.props, "show_home_icon", false);
    let has_color = node
        .props
        .get("color")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|s| !s.is_empty());
    let color_style = if has_color {
        " style=\"color:var(--od-bc-color);\""
    } else {
        ""
    };
    let mut ol_cls = vec![
        "flex".to_owned(),
        "flex-wrap".to_owned(),
        "items-center".to_owned(),
        "gap-1.5".to_owned(),
        "text-xs".to_owned(),
        "text-od-muted".to_owned(),
        "list-none".to_owned(),
        "p-0".to_owned(),
    ];
    ol_cls.extend(align_justify_classes(&node.props, "left"));

    let mut lis = String::new();
    for (idx, item) in crumbs.iter().enumerate() {
        if idx > 0 {
            let sep_html = match separator {
                "slash" => "<span aria-hidden=\"true\">/</span>".to_owned(),
                "dot" => "<span aria-hidden=\"true\">•</span>".to_owned(),
                _ => register_core_icon_in_budget(budget, "chevron-right", Some("w-3.5 h-3.5"))?
                    .unwrap_or_else(|| "/".to_owned()),
            };
            lis.push_str(&format!("<li class=\"inline-flex items-center\" aria-hidden=\"true\">{sep_html}</li>"));
        }
        let home_icon = if idx == 0 && show_home_icon {
            register_core_icon_in_budget(budget, "home", Some("w-3.5 h-3.5 inline-block mr-1"))?
                .unwrap_or_default()
        } else {
            String::new()
        };
        let label = format!("{home_icon}{}", escape_html(&item.name));
        if item.is_current || item.url.as_deref().is_none_or(|u| !valid_safe_url(u)) {
            let cur_attr = if item.is_current { " aria-current=\"page\" class=\"font-medium text-od-text\"" } else { "" };
            lis.push_str(&format!("<li{cur_attr}>{label}</li>"));
        } else if let Some(url) = item.url.as_deref() {
            lis.push_str(&format!(
                "<li><a href=\"{}\" class=\"hover:text-od-text\">{label}</a></li>",
                escape_html(url),
            ));
        }
    }

    Ok(Some(format!(
        "<nav class=\"od-breadcrumbs\" aria-label=\"{}\"{color_style}><ol class=\"{}\">{lis}</ol></nav>",
        escape_html(l10n.breadcrumb_nav.as_ref()),
        ol_cls.join(" "),
    )))
}

pub(super) fn render_chrome_block(
    node: &BlockNode,
    scope: &str,
    td: Option<&TemplateRenderData<'_>>,
    is_preview: bool,
    budget: &mut RenderBudget,
) -> ApiResult<Option<Option<String>>> {
    let locale = budget.locale.clone();
    let l10n = core_l10n(&locale);
    match node.block_id.as_str() {
        "core/site-brand" => Ok(Some(render_site_brand(node, td, is_preview))),
        "core/language-switcher" => render_language_switcher(node, td, &locale, is_preview, budget)
            .map(Some)
            .map_err(ApiError::bad_request),
        "core/menu" => render_menu_block(node, scope, td, &l10n, is_preview, budget)
            .map(Some)
            .map_err(ApiError::bad_request),
        "core/breadcrumbs" => render_breadcrumbs_block(node, td, &l10n, is_preview, budget)
            .map(Some)
            .map_err(ApiError::bad_request),
        _ => Ok(None),
    }
}
