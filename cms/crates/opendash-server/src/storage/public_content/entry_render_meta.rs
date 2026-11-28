use super::*;

pub fn format_public_canonical_url(public_origin: Option<&str>, path: &str) -> String {
    let clean_path = match path.trim() {
        p if p.starts_with("//") => "/",
        p if p.starts_with('/') => p,
        _ => "/",
    };
    let origin = public_origin
        .map(|s| s.trim().trim_end_matches('/'))
        .unwrap_or("");
    if origin.is_empty() {
        clean_path.to_owned()
    } else {
        format!("{origin}{clean_path}")
    }
}

pub(in crate::storage) async fn populate_entry_render_budget(
    tx: &mut Transaction<'_, Sqlite>,
    render_budget: &mut crate::page::RenderBudget,
    entry_locale: &str,
    entry_title: &str,
    canonical_path: &str,
) -> ApiResult<()> {
    let pub_settings = crate::storage::settings::get_public_in_conn(&mut **tx).await?;
    render_budget.locale = entry_locale.to_owned();
    render_budget.page_title = Some(entry_title.to_owned());
    render_budget.page_canonical_url = Some(format_public_canonical_url(
        pub_settings.public_origin.as_deref(),
        canonical_path,
    ));
    Ok(())
}
