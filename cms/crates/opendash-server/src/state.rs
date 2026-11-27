use std::sync::Arc;

use sqlx::SqlitePool;

use crate::{hooks::HookDispatcher, media::MediaStore, plugin::PluginManager};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub ui_locale: Arc<std::sync::RwLock<Option<String>>>,
    pub plugin_manager: Arc<PluginManager>,
    pub hooks: Arc<HookDispatcher>,
    pub media_store: Arc<MediaStore>,
    pub content_registry: Arc<
        tokio::sync::RwLock<
            opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>,
        >,
    >,
    pub coordinator: Arc<crate::operations::SiteCoordinator>,
    pub backup_registry: Arc<crate::backup::BackupRuntimeRegistry>,
    pub block_template_cache: Arc<crate::page::BlockTemplateCache>,
    pub site_cache_epoch: Arc<crate::cache::SiteCacheEpoch>,
    pub render_cache: Arc<crate::cache::ExtensionRenderCache>,
    pub catalog_registry: Arc<crate::l10n::CatalogRegistry>,
    pub recovery: crate::config::RecoveryState,
    pub runtime_config: crate::config::RuntimeConfigV1,
}

impl axum::extract::FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<PluginManager> {
    fn from_ref(state: &AppState) -> Self {
        state.plugin_manager.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<HookDispatcher> {
    fn from_ref(state: &AppState) -> Self {
        state.hooks.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<MediaStore> {
    fn from_ref(state: &AppState) -> Self {
        state.media_store.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::operations::SiteCoordinator> {
    fn from_ref(state: &AppState) -> Self {
        state.coordinator.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::page::BlockTemplateCache> {
    fn from_ref(state: &AppState) -> Self {
        state.block_template_cache.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::cache::SiteCacheEpoch> {
    fn from_ref(state: &AppState) -> Self {
        state.site_cache_epoch.clone()
    }
}

impl axum::extract::FromRef<AppState> for Arc<crate::cache::ExtensionRenderCache> {
    fn from_ref(state: &AppState) -> Self {
        state.render_cache.clone()
    }
}

impl axum::extract::FromRef<AppState>
    for Arc<
        tokio::sync::RwLock<
            opendash_core::registry::RegistryCatalog<opendash_core::content::ContentTypeDefinition>,
        >,
    >
{
    fn from_ref(state: &AppState) -> Self {
        state.content_registry.clone()
    }
}
