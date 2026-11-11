//! Page use cases shared by preview, save and public rendering.
use crate::{
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    storage::{
        builder::HomeState,
        builder_binding::{BindingResult, BindingResults},
        page::load_theme_block,
        theme::{self as theme_storage, HomeSnapshot, SelectedTemplate},
    },
    theme::{self, Block},
};
use opendash_core::{
    builder::{
        BindingTarget, BuilderDocument, ElementContent, KnownElement, Responsive, ResponsiveStyle,
        TextAlign,
    },
    hooks::HookEvent,
    id::valid_package_id,
};
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;

pub struct RenderedBlock {
    pub html: String,
    pub visibility: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BuilderDiagnostic {
    pub node_id: String,
    pub code: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RenderedLayout {
    pub html: String,
    pub diagnostics: Vec<BuilderDiagnostic>,
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Clone, Copy)]
enum ContainerKind {
    Section,
    Row,
    Column,
}

fn responsive_declarations(style: Option<&ResponsiveStyle>, kind: ContainerKind) -> String {
    let mut css = String::new();
    let Some(style) = style else { return css };
    if let Some(value) = style.padding_px {
        css.push_str(&format!("padding:{value}px;"));
    }
    if matches!(kind, ContainerKind::Row) {
        if let Some(value) = style.gap_px {
            css.push_str(&format!("gap:{value}px;"));
        }
        if let Some(value) = style.columns_per_row {
            css.push_str(&format!(
                "grid-template-columns:repeat({value},minmax(0,1fr));"
            ));
        }
    }
    if matches!(kind, ContainerKind::Column) {
        if let Some(value) = style.text_align {
            let value = match value {
                TextAlign::Start => "start",
                TextAlign::Center => "center",
                TextAlign::End => "end",
            };
            css.push_str(&format!("text-align:{value};"));
        }
    }
    css
}

fn push_scoped_rule(target: &mut String, id: &str, properties: &str) {
    // These IDs are validated on save; also refuse invalid historical IDs
    // so no untrusted value can enter the CSS selector or style element.
    if !properties.is_empty() && valid_package_id(id) {
        target.push_str(&format!(
            "main[data-builder-version=\"1\"] [data-node-id=\"{id}\"]{{{properties}}}"
        ));
    }
}

/// One CSS source is used for both authenticated Preview and public HTML.
/// The range queries preserve exact CSS-pixel boundaries, including fractions.
fn builder_responsive_css(document: &BuilderDocument) -> String {
    let mut desktop = String::new();
    let mut tablet = String::new();
    let mut mobile = String::new();
    let mut append = |id: &str, responsive: Option<&Responsive>, kind: ContainerKind| {
        let Some(responsive) = responsive else { return };
        let mut base = String::new();
        if matches!(kind, ContainerKind::Row) && responsive.opts_into_grid() {
            base.push_str("display:grid;grid-template-columns:repeat(1,minmax(0,1fr));gap:0px;");
        }
        base.push_str(&responsive_declarations(responsive.desktop.as_ref(), kind));
        push_scoped_rule(&mut desktop, id, &base);
        push_scoped_rule(
            &mut tablet,
            id,
            &responsive_declarations(responsive.tablet.as_ref(), kind),
        );
        push_scoped_rule(
            &mut mobile,
            id,
            &responsive_declarations(responsive.mobile.as_ref(), kind),
        );
    };
    for section in &document.sections {
        append(
            &section.id,
            section.responsive.as_ref(),
            ContainerKind::Section,
        );
        for row in &section.children {
            append(&row.id, row.responsive.as_ref(), ContainerKind::Row);
            for column in &row.children {
                append(
                    &column.id,
                    column.responsive.as_ref(),
                    ContainerKind::Column,
                );
            }
        }
    }
    if desktop.is_empty() && tablet.is_empty() && mobile.is_empty() {
        return String::new();
    }
    let mut css = format!("<style data-builder-responsive>{desktop}");
    if !tablet.is_empty() {
        css.push_str(&format!("@media (width < 1024px){{{tablet}}}"));
    }
    if !mobile.is_empty() {
        css.push_str(&format!("@media (width < 768px){{{mobile}}}"));
    }
    css.push_str("</style>");
    css
}

/// Template selection comes from one storage snapshot. Unsupported legacy
/// nodes are invisible in public markup but remain intact in Builder JSON.
/// Only renderer-known core elements can add public media references.
#[cfg(test)]
pub fn render_builder(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
) -> ApiResult<RenderedLayout> {
    render_builder_resolved(state, document, is_preview, &BindingResults::new())
}

/// All bound values are obtained by the storage layer under Preview/public ACL.
/// The renderer never reads SQL or treats rich text as raw markup.
pub fn render_builder_resolved(
    state: &HomeState,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
) -> ApiResult<RenderedLayout> {
    // Keep the original v1 main/section child structure intact; the optional
    // stylesheet precedes the root and every selector is scoped to that root.
    let mut html = builder_responsive_css(document);
    html.push_str("<main data-builder-version=\"1\">");
    let mut diagnostics = Vec::new();
    for section in &document.sections {
        html.push_str(&format!(
            "<section data-node-id=\"{}\">",
            escape_html(&section.id)
        ));
        for row in &section.children {
            html.push_str(&format!(
                "<div class=\"builder-row\" data-node-id=\"{}\">",
                escape_html(&row.id)
            ));
            for column in &row.children {
                html.push_str(&format!(
                    "<div class=\"builder-column\" data-node-id=\"{}\">",
                    escape_html(&column.id)
                ));
                for element in &column.children {
                    let mut materialized = None;
                    if let ElementContent::Known(known) = &element.content {
                        if let Some(binding) = known.binding() {
                            let value = match bindings.get(&element.id) {
                                Some(BindingResult::Value(value)) => value,
                                result => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: match result {
                                            Some(BindingResult::Incompatible) => {
                                                "binding_incompatible"
                                            }
                                            Some(BindingResult::UrlRejected) => {
                                                "binding_url_rejected"
                                            }
                                            _ => "binding_unavailable",
                                        },
                                    });
                                    continue;
                                }
                            };
                            let mut resolved = known.clone();
                            match (&mut resolved, binding.target) {
                                (KnownElement::Heading(props), BindingTarget::Text) => {
                                    props.text = value.clone()
                                }
                                (KnownElement::Text(props), BindingTarget::Text) => {
                                    props.text = value.clone()
                                }
                                (KnownElement::Image(props), BindingTarget::MediaId) => {
                                    props.media_id = value.clone()
                                }
                                (KnownElement::Image(props), BindingTarget::Alt) => {
                                    props.alt = value.clone()
                                }
                                (KnownElement::Button(props), BindingTarget::Label) => {
                                    props.label = value.clone()
                                }
                                (KnownElement::Button(props), BindingTarget::Url) => {
                                    props.url = value.clone()
                                }
                                _ => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "binding_incompatible",
                                    });
                                    continue;
                                }
                            }
                            materialized = Some(ElementContent::Known(resolved));
                        }
                    }
                    let rendered = match materialized.as_ref().unwrap_or(&element.content) {
                        ElementContent::Known(KnownElement::Heading(props)) => Some(format!(
                            "<h{level}>{text}</h{level}>",
                            level = props.level,
                            text = escape_html(&props.text)
                        )),
                        ElementContent::Known(KnownElement::Text(props)) => {
                            Some(format!("<p>{}</p>", escape_html(&props.text)))
                        }
                        ElementContent::Known(KnownElement::Image(props)) => {
                            let path = if is_preview {
                                "/api/admin/media"
                            } else {
                                "/media"
                            };
                            Some(format!(
                                "<img src=\"{path}/{id}/file\" alt=\"{alt}\" loading=\"lazy\">",
                                id = escape_html(&props.media_id),
                                alt = escape_html(&props.alt)
                            ))
                        }
                        ElementContent::Known(KnownElement::Button(props)) => Some(format!(
                            "<a href=\"{}\" rel=\"noopener noreferrer\">{}</a>",
                            escape_html(&props.url),
                            escape_html(&props.label)
                        )),
                        ElementContent::Known(KnownElement::Legacy(props)) => {
                            match state.themes.get(&props.block_id) {
                                None => {
                                    diagnostics.push(BuilderDiagnostic {
                                        node_id: element.id.clone(),
                                        code: "descriptor_missing",
                                    });
                                    None
                                }
                                Some(definition) => {
                                    let selected = theme_storage::select_template(
                                        &props.block_id,
                                        definition.template.clone(),
                                        state.active_id.as_deref(),
                                        state.active_alias.as_deref(),
                                        state.active_template.clone(),
                                    );
                                    if let Some(template) = selected.template {
                                        let block = Block {
                                            kind: props.block_id.clone(),
                                            values: props.values.clone(),
                                        };
                                        if block
                                            .validate_against_fields(&definition.fields)
                                            .is_err()
                                        {
                                            diagnostics.push(BuilderDiagnostic {
                                                node_id: element.id.clone(),
                                                code: "legacy_fields_invalid",
                                            });
                                            None
                                        } else {
                                            Some(
                                                theme::render(
                                                    &template,
                                                    &definition.fields,
                                                    &block,
                                                )
                                                .map_err(ApiError::bad_request)?,
                                            )
                                        }
                                    } else {
                                        diagnostics.push(BuilderDiagnostic {
                                            node_id: element.id.clone(),
                                            code: "unsupported_by_theme",
                                        });
                                        None
                                    }
                                }
                            }
                        }
                        ElementContent::Unresolved(_) => {
                            diagnostics.push(BuilderDiagnostic {
                                node_id: element.id.clone(),
                                code: "descriptor_missing",
                            });
                            None
                        }
                    };
                    if let Some(fragment) = rendered {
                        html.push_str(&format!(
                            "<div class=\"builder-element\" data-node-id=\"{}\">{fragment}</div>",
                            escape_html(&element.id)
                        ));
                    }
                }
                html.push_str("</div>");
            }
            html.push_str("</div>");
        }
        html.push_str("</section>");
    }
    html.push_str("</main>");
    Ok(RenderedLayout { html, diagnostics })
}

pub fn render_legacy_state(state: &HomeState) -> ApiResult<Option<RenderedBlock>> {
    let Some(block) = &state.legacy else {
        return Ok(None);
    };
    let definition = state
        .themes
        .get(&block.kind)
        .ok_or_else(|| ApiError::internal("Missing legacy theme descriptor"))?;
    let selected = theme_storage::select_template(
        &block.kind,
        definition.template.clone(),
        state.active_id.as_deref(),
        state.active_alias.as_deref(),
        state.active_template.clone(),
    );
    render_selected(block, &definition.fields, selected).map(Some)
}

pub fn layout_hook_payload(document: &BuilderDocument, version: i64) -> serde_json::Value {
    let elements: Vec<_> = document
        .sections
        .iter()
        .flat_map(|s| &s.children)
        .flat_map(|r| &r.children)
        .flat_map(|c| &c.children)
        .collect();
    if let [only] = elements.as_slice() {
        if let ElementContent::Known(KnownElement::Legacy(props)) = &only.content {
            return json!({"block_type": props.block_id});
        }
    }
    json!({"block_type": "core/layout", "layout_version": version})
}

pub async fn dispatch_saved_hook(hooks: &HookDispatcher, payload: serde_json::Value) {
    let report = hooks.dispatch(HookEvent::PageAfterSave, payload).await;
    for outcome in &report.outcomes {
        if let Some(error) = &outcome.error {
            eprintln!(
                "Saved page; hook {} failed ({}): {error}",
                outcome.handler_id, report.envelope.correlation_id
            );
        }
    }
}

fn render_selected(
    block: &Block,
    fields: &theme::Fields,
    selected: SelectedTemplate,
) -> ApiResult<RenderedBlock> {
    // Always validate against the author's original block schema, including when
    // the active theme does not support it. Incompatible does not mean invalid.
    block
        .validate_against_fields(fields)
        .map_err(ApiError::bad_request)?;
    let html = match selected.template {
        Some(template) => theme::render(&template, fields, block).map_err(ApiError::bad_request)?,
        None => "<main></main>".to_string(), // Safe empty content; never borrow a stale template.
    };
    Ok(RenderedBlock {
        html,
        visibility: selected.visibility,
    })
}

pub fn render_saved(snapshot: HomeSnapshot) -> ApiResult<RenderedBlock> {
    render_selected(
        &snapshot.block,
        &snapshot.original_fields,
        snapshot.selected,
    )
}

pub async fn render_block(db: &SqlitePool, block: &Block) -> ApiResult<RenderedBlock> {
    let (original_template, fields) = load_theme_block(db, &block.kind).await?;
    let (active_id, active_alias, active_template) = theme_storage::active_selection(db).await?;
    let selected = theme_storage::select_template(
        &block.kind,
        original_template,
        active_id.as_deref(),
        active_alias.as_deref(),
        active_template,
    );
    render_selected(block, &fields, selected)
}

pub async fn save_page(
    db: &SqlitePool,
    hooks: &HookDispatcher,
    block: &Block,
) -> ApiResult<RenderedBlock> {
    // A converted homepage must reject *every* legacy write with 409,
    // including requests whose legacy descriptor no longer resolves.
    // The transaction in save_home_page_and_render repeats the guard under
    // the writer lock to close the race with a concurrent Builder Save.
    crate::storage::builder::legacy_page(db).await?;
    // The source schema is immutable once a package is installed. Validate
    // before acquiring a SQLite write lock; render + homepage write observe
    // one active-theme version inside the same write transaction.
    let (original_template, fields) = load_theme_block(db, &block.kind).await?;
    block
        .validate_against_fields(&fields)
        .map_err(ApiError::bad_request)?;
    let rendered =
        theme_storage::save_home_page_and_render(db, block, original_template, |selected| {
            render_selected(block, &fields, selected)
        })
        .await?;
    // The SQLite autocommit has completed. Notifications cannot invalidate this save.
    dispatch_saved_hook(hooks, json!({ "block_type": block.kind })).await;
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plugin::PluginManager,
        storage::{
            builder::{read as builder_read, save_and_render as builder_save_and_render},
            page::{insert_theme_block, load_home_page},
        },
        theme::{BlockDescriptor, Fields, Theme},
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn save_commits_before_hooks_and_keeps_data_after_handler_failure() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let fields = Fields::from([("title".into(), "Title".into())]);
        insert_theme_block(
            &db,
            &Theme {
                descriptor: BlockDescriptor {
                    id: "sample/banner".into(),
                    fields,
                },
                template: "<h1>{{ values.title }}</h1>".into(),
                content_list_template: None,
                content_single_template: None,
                compatible_block_id: None,
            },
        )
        .await
        .unwrap();
        let block = Block {
            kind: "sample/banner".into(),
            values: Fields::from([("title".into(), "Saved title".into())]),
        };
        let observed = Arc::new(AtomicBool::new(false));
        let mut hooks = HookDispatcher::new(Arc::new(PluginManager::new(db.clone(), false)));
        let observed_by_handler = observed.clone();
        let handler_db = db.clone();
        hooks
            .register_handler(
                "core/check_commit",
                HookEvent::PageAfterSave,
                1,
                move |event| {
                    let db = handler_db.clone();
                    let observed = observed_by_handler.clone();
                    Box::pin(async move {
                        assert_eq!(
                            load_home_page(&db).await.unwrap().unwrap().values["title"],
                            "Saved title"
                        );
                        assert_eq!(event.payload["block_type"], "sample/banner");
                        observed.store(true, Ordering::SeqCst);
                        Err("expected handler failure".into())
                    })
                },
            )
            .unwrap();
        hooks
            .register_handler("core/panic", HookEvent::PageAfterSave, 2, |_| {
                Box::pin(async { panic!("expected handler panic") })
            })
            .unwrap();
        // Preview does not write or dispatch.
        assert!(
            render_block(&db, &block)
                .await
                .unwrap()
                .html
                .contains("Saved title")
        );
        assert!(load_home_page(&db).await.unwrap().is_none());
        assert!(!observed.load(Ordering::SeqCst));
        assert!(
            save_page(&db, &hooks, &block)
                .await
                .unwrap()
                .html
                .contains("Saved title")
        );
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(load_home_page(&db).await.unwrap().unwrap(), block);
        let invalid = Block {
            kind: "missing/block".into(),
            values: block.values.clone(),
        };
        assert!(save_page(&db, &hooks, &invalid).await.is_err());
        assert_eq!(load_home_page(&db).await.unwrap().unwrap(), block);
    }

    #[tokio::test]
    async fn builder_save_commits_before_hook_failure_and_never_exposes_draft() {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../migrations").run(&db).await.unwrap();
        let draft = BuilderDocument::from_json(
            br#"{"schema_version":1,"sections":[{"id":"section-root","children":[{"id":"row-root","children":[{"id":"column-root","children":[{"id":"heading-root","type":"core/text","props":{"text":"Committed"}}]}]}]}]}"#,
        ).unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let mut hooks = HookDispatcher::new(Arc::new(PluginManager::new(db.clone(), false)));
        let observed_copy = observed.clone();
        let observer_db = db.clone();
        hooks
            .register_handler(
                "core/builder_commit_check",
                HookEvent::PageAfterSave,
                1,
                move |event| {
                    let observed = observed_copy.clone();
                    let db = observer_db.clone();
                    Box::pin(async move {
                        let committed = builder_read(&db).await.unwrap();
                        assert_eq!(committed.version, 2);
                        assert!(committed.saved.is_some());
                        assert_eq!(
                            event.payload,
                            json!({"block_type": "core/layout", "layout_version": 2})
                        );
                        observed.store(true, Ordering::SeqCst);
                        Err("expected hook failure after commit".to_string())
                    })
                },
            )
            .unwrap();

        let (version, rendered) =
            builder_save_and_render(&db, &draft, 1, |current, document, bindings| {
                render_builder_resolved(current, document, false, bindings)
            })
            .await
            .unwrap();
        assert_eq!(version, 2);
        assert!(rendered.html.contains("Committed"));
        assert!(!observed.load(Ordering::SeqCst));
        dispatch_saved_hook(&hooks, layout_hook_payload(&draft, version)).await;
        assert!(observed.load(Ordering::SeqCst));
        assert_eq!(builder_read(&db).await.unwrap().saved, Some(draft.clone()));
        let failure = builder_save_and_render(&db, &draft, 1, |current, document, bindings| {
            render_builder_resolved(current, document, false, bindings)
        })
        .await
        .unwrap_err();
        assert_eq!(failure.0, axum::http::StatusCode::PRECONDITION_FAILED);
        assert_eq!(builder_read(&db).await.unwrap().version, 2);
    }

    #[test]
    fn single_legacy_node_uses_original_m3_hook_block_type() {
        let doc = BuilderDocument::from_json(
            br#"{"schema_version":1,"sections":[{"id":"section-root","children":[{"id":"row-root","children":[{"id":"column-root","children":[{"id":"block-root","type":"legacy/block","props":{"block_id":"sample/banner","values":{"title":"Secret draft"}}}]}]}]}]}"#,
        ).unwrap();
        assert_eq!(
            layout_hook_payload(&doc, 42),
            json!({"block_type": "sample/banner"})
        );
        let empty = BuilderDocument::empty();
        assert_eq!(
            layout_hook_payload(&empty, 3),
            json!({"block_type": "core/layout", "layout_version": 3})
        );
    }
}
