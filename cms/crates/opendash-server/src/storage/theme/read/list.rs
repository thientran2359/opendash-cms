use super::*;

/// Source block ID is never overwritten or silently translated. NULL active
pub async fn list(db: &SqlitePool) -> ApiResult<ThemeList> {
    // All fields (including revision visibility and CAS selection) are one read
    // snapshot. A concurrent activate/install cannot interleave two GET fields.
    let mut tx = db.begin().await?;
    let (revision_id, theme_id, version, source_format): (
        Option<String>,
        Option<String>,
        i64,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT a.revision_id, r.theme_id, a.version, r.source_format
         FROM theme_active_selection a
         LEFT JOIN theme_revisions r ON r.revision_id = a.revision_id
         WHERE a.id = 1",
    )
    .fetch_one(&mut *tx)
    .await?;
    let selection = ActiveThemeSelection {
        revision_id,
        theme_id,
        version,
        source_format: source_format
            .as_deref()
            .map(ThemeSourceFormat::from_db)
            .transpose()?,
    };
    let raw_revisions: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT revision_id, theme_id, version, revision_digest, source_format
         FROM theme_revisions ORDER BY theme_id ASC, version ASC, revision_id ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let revisions = raw_revisions
        .into_iter()
        .map(
            |(revision_id, theme_id, version, revision_digest, source_format)| {
                Ok(ThemeRevisionSummary {
                    revision_id,
                    theme_id,
                    version,
                    revision_digest,
                    source_format: ThemeSourceFormat::from_db(&source_format)?,
                })
            },
        )
        .collect::<ApiResult<Vec<_>>>()?;
    Ok(ThemeList {
        version,
        revisions,
        selection,
    })
}
