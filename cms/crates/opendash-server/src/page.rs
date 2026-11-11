//! Page use cases shared by preview, save and public rendering.
use crate::{
    error::{ApiError, ApiResult},
    hooks::HookDispatcher,
    storage::{
        page::load_theme_block,
        theme::{self as theme_storage, HomeSnapshot, SelectedTemplate},
    },
    theme::{self, Block},
};
use opendash_core::hooks::HookEvent;
use serde_json::json;
use sqlx::SqlitePool;

pub struct RenderedBlock {
    pub html: String,
    pub visibility: &'static str,
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
    let report = hooks
        .dispatch(
            HookEvent::PageAfterSave,
            json!({ "block_type": block.kind }),
        )
        .await;
    for outcome in &report.outcomes {
        if let Some(error) = &outcome.error {
            eprintln!(
                "Saved page; hook {} failed ({}): {error}",
                outcome.handler_id, report.envelope.correlation_id
            );
        }
    }
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        plugin::PluginManager,
        storage::page::{insert_theme_block, load_home_page},
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
}
