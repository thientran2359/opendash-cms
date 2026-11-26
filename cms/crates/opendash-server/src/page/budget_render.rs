use super::*;

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_annotations(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
) -> ApiResult<RenderedLayout> {
    let scope = if pattern_document { "pat_0" } else { "entry" };
    let mut budget = RenderBudget::new();
    render_builder_with_budget(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        &mut budget,
        BlockTemplateCache::global(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_builder_with_budget(
    state: &BuilderRenderContext,
    document: &BuilderDocument,
    is_preview: bool,
    bindings: &BindingResults,
    lists: &ListResults,
    forms: &ContactFormResults,
    context: Option<&BuilderTemplateContext<'_>>,
    patterns: &PatternResults,
    pattern_document: bool,
    annotation_scope: RenderAnnotationScope,
    scope: &str,
    budget: &mut RenderBudget,
    cache: &BlockTemplateCache,
) -> ApiResult<RenderedLayout> {
    render_builder_with_budget_and_base_path(
        state,
        document,
        is_preview,
        bindings,
        lists,
        forms,
        context,
        patterns,
        pattern_document,
        annotation_scope,
        scope,
        "/",
        budget,
        cache,
    )
}
