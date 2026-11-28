use super::*;

pub(crate) use crate::{
    error::{ApiError, ApiResult},
    public_context::CanonicalUrlBuilder,
    storage::builder::{BuilderRenderContext, ListResults, ResolvedList},
};

pub(crate) use opendash_core::{
    builder::{BlockNode, BuilderDocument},
    id::valid_package_id,
};

pub(crate) use serde::Serialize;

pub use crate::template_sec::RenderBudget;

#[derive(Debug, Serialize)]
pub struct BuilderDiagnostic {
    pub node_id: String,
    pub code: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RenderedLayout {
    #[serde(skip_serializing)]
    pub has_active_contact_form: bool,
    #[serde(skip_serializing)]
    pub needs_blocks_js: bool,
    #[serde(skip_serializing)]
    pub used_entry_content: bool,
    #[serde(skip_serializing)]
    pub has_popup: bool,
    pub html: String,
    pub diagnostics: Vec<BuilderDiagnostic>,
}

/// The S02 public resolver supplies already sanitized entry HTML and verified
/// active theme parts. Ordinary page Builder calls have no template context.
pub struct BuilderTemplateContext<'a> {
    pub content_html: Option<&'a str>,
    pub parts: &'a std::collections::BTreeMap<String, String>,
    pub search: Option<BuilderSearchData<'a>>,
    pub template_data: Option<&'a crate::content_render::TemplateRenderData<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderAnnotationScope {
    #[default]
    None,
    TemplateAll,
    EntryBodyOnly,
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_budget_and_base_path(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
    list_base_path: &str,
    budget: &mut RenderBudget,
    cache: &BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    super::node_renderer::render_document(
        state,
        document,
        is_preview,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        list_base_path,
        budget,
        cache,
    )
}
