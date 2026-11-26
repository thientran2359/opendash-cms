use super::*;

pub fn image_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        if let Some(bg_media) = section
            .style
            .as_ref()
            .and_then(|s| s.background_media_id.as_deref())
            .filter(|id| !id.is_empty())
        {
            ids.insert(bg_media);
        }
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Image(image)) = &element.content {
                        if !image.binding.as_ref().is_some_and(|b| {
                            b.target == opendash_core::builder::BindingTarget::MediaId
                        }) {
                            ids.insert(image.media_id.as_str());
                        }
                    }
                }
            }
        }
    }
    ids
}

pub fn video_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Video(video)) = &element.content {
                        ids.insert(video.media_id.as_str());
                    }
                }
            }
        }
    }
    ids
}

pub fn poster_image_ids(document: &BuilderDocument) -> BTreeSet<&str> {
    let mut ids = BTreeSet::new();
    for section in &document.sections {
        for row in &section.children {
            for column in &row.children {
                for element in &column.children {
                    if let ElementContent::Known(KnownElement::Video(video)) = &element.content
                        && let Some(poster) = video.poster_image_id.as_deref()
                    {
                        ids.insert(poster);
                    }
                }
            }
        }
    }
    ids
}

/// Public ACL projection of a Builder document. Unknown and inactive block
/// schemas never contribute media refs; raw saved values remain untouched so
/// the separate delete-pin scanner can still protect their assets.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DocumentMediaRefs {
    pub image_ids: BTreeSet<String>,
    pub video_ids: BTreeSet<String>,
    pub poster_image_ids: BTreeSet<String>,
    pub any_media_ids: BTreeSet<String>,
    pub file_ids: BTreeSet<String>,
}

pub(in crate::storage) fn has_nested_pattern_or_list_with_schemas(
    document: &BuilderDocument,
    schemas: &RegistryCatalog<RegisteredBlockSchema>,
) -> bool {
    document.sections.iter().any(|section| {
        section.children.iter().any(|row| {
            row.children.iter().any(|column| {
                column
                    .children
                    .iter()
                    .any(|element| match &element.content {
                        ElementContent::Known(
                            KnownElement::PatternRef(_) | KnownElement::List(_),
                        ) => true,
                        ElementContent::Known(KnownElement::PluginBlock(props)) => {
                            schemas.get(&props.block_id).is_some_and(|entry| {
                                entry.status == ProviderStatus::Active
                                    && entry.declaration_state == DeclarationState::Declared
                                    && entry
                                        .descriptor
                                        .schema
                                        .has_non_empty_pattern_source_value(&props.values)
                            })
                        }
                        ElementContent::Unresolved(raw) => matches!(
                            raw.get("type").and_then(Value::as_str),
                            Some("core/pattern_ref" | "core/list")
                        ),
                        _ => false,
                    })
            })
        })
    })
}

/// Builder documents are site-owned entry content. Their only version is the
/// enclosing content entry CAS token; shared site templates are independent.
#[derive(Debug, Clone, Serialize)]
pub struct EntryBuilderOpen {
    pub entry_id: i64,
    pub type_key: String,
    pub content_mode: ContentMode,
    pub version: i64,
    pub updated_at: String,
    pub document: Option<BuilderDocument>,
    pub publication_status: String,
    pub publication_policy: ContentPublication,
    pub diagnostics: Vec<BlockDiagnostic>,
    pub had_freeform_wrap: bool,
    pub had_recovery: bool,
}

pub(in crate::storage) struct EntryBuilderSnapshot {
    pub(in crate::storage) opened: EntryBuilderOpen,
    pub(in crate::storage) schema: EffectiveContentSchema,
    pub(in crate::storage) locale: String,
    pub(in crate::storage) translation_group_id: String,
    pub(in crate::storage) title: String,
    pub(in crate::storage) slug: String,
    pub(in crate::storage) values: BTreeMap<String, Value>,
    pub(in crate::storage) term_ids: Vec<i64>,
    pub(in crate::storage) raw_body: Option<String>,
    pub(in crate::storage) raw_body_has_blocks: bool,
    pub(in crate::storage) page_layout_key: Option<String>,
    pub(in crate::storage) custom_template_key: Option<String>,
    pub(in crate::storage) trashed_at: Option<String>,
}

pub(in crate::storage) fn actor_field_role(actor: &SessionContext) -> FieldRole {
    match actor.user.role {
        Role::Admin => FieldRole::Admin,
        Role::Editor => FieldRole::Editor,
    }
}

pub(in crate::storage) fn check_entry_body_edit_acl(
    schema: &EffectiveContentSchema,
    actor: &SessionContext,
) -> ApiResult<()> {
    let Some(body_field) = schema.effective_fields.get("body") else {
        return Err(ApiError::forbidden(
            "Loại nội dung không có trường 'body' hợp lệ cho Trình dựng trang.",
        ));
    };
    if body_field.definition.field_type != ContentFieldType::RichText {
        return Err(ApiError::forbidden(
            "Trường 'body' phải có kiểu rich_text để dùng Trình dựng trang.",
        ));
    }
    if actor_field_role(actor) < body_field.definition.min_edit_role {
        return Err(ApiError::forbidden(
            "Bạn không có quyền chỉnh sửa nội dung 'body'.",
        ));
    }
    Ok(())
}

pub(in crate::storage) fn body_allows_public_refs(schema: &EffectiveContentSchema) -> bool {
    schema.effective_fields.get("body").is_some_and(|fe| {
        fe.definition.field_type == ContentFieldType::RichText
            && fe.definition.public_exposure == PublicExposure::Public
            && fe.definition.min_view_role == FieldRole::Editor
    })
}

pub(in crate::storage) fn visible_builder_field(
    schema: &EffectiveContentSchema,
    key: &str,
    role: FieldRole,
) -> bool {
    schema.effective_fields.get(key).is_some_and(|field| {
        field.definition.min_view_role <= role
            && field.definition.public_exposure == PublicExposure::Public
            && field.effective_status == EffectiveFieldStatus::Active
    })
}

#[derive(Debug)]
pub enum EntryBuilderMutationError {
    Api(ApiError),
    Veto(ContentBeforeSaveVeto),
}

impl From<ApiError> for EntryBuilderMutationError {
    fn from(err: ApiError) -> Self {
        Self::Api(err)
    }
}

impl From<sqlx::Error> for EntryBuilderMutationError {
    fn from(err: sqlx::Error) -> Self {
        Self::Api(err.into())
    }
}

pub(in crate::storage) enum EntryBuilderMutation<'a> {
    Save(&'a BuilderDocument),
    Reset,
}

impl EntryBuilderMutation<'_> {
    pub(in crate::storage) fn operation(&self) -> &'static str {
        match self {
            Self::Save(_) => "save",
            Self::Reset => "reset",
        }
    }
}

pub(in crate::storage) fn check_entry_builder_cas(
    current: &EntryBuilderSnapshot,
    expected_version: i64,
    mutation: &EntryBuilderMutation<'_>,
) -> ApiResult<()> {
    if current.opened.version != expected_version {
        return Err(ApiError::precondition_failed("Entry version changed."));
    }
    if current.opened.type_key != "pattern" && current.trashed_at.is_some() {
        return Err(ApiError::conflict(
            "entry_trashed: Nội dung đang ở trong Thùng rác và không thể chỉnh sửa trong Trình dựng trang.",
        ));
    }
    if matches!(mutation, EntryBuilderMutation::Reset) {
        let has_builder_doc = if current.opened.type_key == "pattern" {
            current.opened.document.is_some()
        } else {
            current.raw_body_has_blocks
        };
        if !has_builder_doc {
            return Err(ApiError::not_found("No saved Content Builder document."));
        }
    }
    Ok(())
}

/// Startup cleanup under `SiteCoordinator` lock: moves any non-pattern rows
/// left in `entry_builder_documents` (e.g. Spec 2 demo rows) into unified
/// `content_entries.values_json["body"]`, re-syncs media/pattern refs, and
/// removes those non-pattern rows from `entry_builder_documents`.
pub async fn migrate_demo_entry_builder_docs_on_startup(db: &SqlitePool) -> ApiResult<usize> {
    let mut tx = db.begin().await?;
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT e.id, e.content_type_key, e.values_json, d.document_json
           FROM entry_builder_documents d
           JOIN content_entries e ON e.id = d.entry_id
          WHERE e.content_type_key != 'pattern'
          ORDER BY d.entry_id ASC",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut migrated = 0usize;
    for (entry_id, type_key, values_json, doc_json) in rows {
        let mut values: BTreeMap<String, Value> =
            serde_json::from_str(&values_json).unwrap_or_default();
        let existing_has_blocks = values
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(opendash_core::builder_blocks::contains_blocks);
        let parsed_doc = BuilderDocument::from_json(doc_json.as_bytes()).ok();
        let mut synced_doc = parsed_doc.clone();
        if !existing_has_blocks {
            if let Some(doc) = parsed_doc.as_ref()
                && let Ok(blocks) =
                    opendash_core::builder_blocks::serialize_builder_document_to_blocks(doc)
            {
                values.insert("body".to_owned(), Value::String(blocks));
                let updated_json = serde_json::to_string(&values)?;
                sqlx::query("UPDATE content_entries SET values_json = ? WHERE id = ?")
                    .bind(updated_json)
                    .bind(entry_id)
                    .execute(&mut *tx)
                    .await?;
            }
        } else if let Some(raw_body) = values.get("body").and_then(Value::as_str) {
            synced_doc = Some(
                opendash_core::builder_blocks::parse_blocks_lenient_or_recover(raw_body).document,
            );
        }
        let allow_public =
            crate::storage::content::load_effective_content_schema_in_tx(&mut *tx, &type_key)
                .await?
                .is_some_and(|schema| body_allows_public_refs(&schema));
        replace_entry_builder_refs(
            &mut tx,
            entry_id,
            if allow_public {
                synced_doc.as_ref()
            } else {
                None
            },
        )
        .await?;
        sqlx::query("DELETE FROM entry_builder_documents WHERE entry_id = ?")
            .bind(entry_id)
            .execute(&mut *tx)
            .await?;
        migrated += 1;
    }
    tx.commit().await?;
    crate::cache::SiteCacheEpoch::global().bump();
    Ok(migrated)
}

pub async fn save_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    document: &BuilderDocument,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    mutate_entry_builder(
        db,
        actor,
        hooks,
        type_key,
        id,
        expected_version,
        EntryBuilderMutation::Save(document),
    )
    .await
}

pub async fn reset_entry_builder(
    db: &SqlitePool,
    actor: &SessionContext,
    hooks: &HookDispatcher,
    type_key: &str,
    id: i64,
    expected_version: i64,
    confirmed: bool,
) -> Result<EntryBuilderOpen, EntryBuilderMutationError> {
    if !confirmed {
        return Err(ApiError::bad_request("Reset requires confirmation.").into());
    }
    mutate_entry_builder(
        db,
        actor,
        hooks,
        type_key,
        id,
        expected_version,
        EntryBuilderMutation::Reset,
    )
    .await
}
