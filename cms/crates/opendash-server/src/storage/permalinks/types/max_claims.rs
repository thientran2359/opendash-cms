use super::*;

pub(in crate::storage) const MAX_CLAIMS: usize = 200_000;

pub const MAX_REDIRECTS: usize = 200_000;

pub(in crate::storage) const MAX_CONFLICT_ITEMS: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermalinkConflictItem {
    pub path: String,
    pub new_target: String,
    pub existing_target: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PermalinkConflictError {
    pub error: String,
    pub total_conflicts: usize,
    pub truncated: bool,
    pub conflicts: Vec<PermalinkConflictItem>,
}

#[derive(Debug)]
pub enum PermalinkPutError {
    Api(ApiError),
    Conflicts(PermalinkConflictError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermalinkSettingsInput {
    pub post_structure: String,
    pub content_type_bases: BTreeMap<String, Option<String>>,
    pub taxonomy_bases: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermalinkSettings {
    pub version: i64,
    pub post_structure: String,
    pub content_type_bases: BTreeMap<String, Option<String>>,
    pub taxonomy_bases: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicRouteTarget {
    Home {
        locale: String,
    },
    Entry {
        id: i64,
        locale: String,
        is_home: bool,
    },
    Term {
        id: i64,
        locale: String,
    },
    Archive {
        type_key: String,
        locale: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedPublicRoute {
    pub target: PublicRouteTarget,
    pub canonical_path: String,
    pub redirect: bool,
}

/// An immutable-owner package declaration. `None` withdraws the default base,
/// but never creates or overwrites a site-level selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRewriteDeclaration {
    pub resource_kind: &'static str,
    pub resource_key: String,
    pub rewrite_base: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::storage) struct ProposedClaim {
    pub(in crate::storage) kind: String,
    pub(in crate::storage) id: String,
    pub(in crate::storage) locale: String,
    pub(in crate::storage) family: Option<String>,
    pub(in crate::storage) generated: bool,
}

#[derive(Debug)]
pub(in crate::storage) struct DynamicPattern {
    pub(in crate::storage) prefix: Vec<String>,
    pub(in crate::storage) seg_count: usize,
    pub(in crate::storage) kind: String,
    pub(in crate::storage) family: String,
    pub(in crate::storage) locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::storage) struct HistoricalOwner {
    pub(in crate::storage) claim: Claim,
    pub(in crate::storage) origin: String,
}

pub(in crate::storage) struct ConflictCollector {
    pub(in crate::storage) collect_all: bool,
    pub(in crate::storage) total_conflicts: usize,
    pub(in crate::storage) items: Vec<PermalinkConflictItem>,
    pub(in crate::storage) first_error: Option<String>,
}

impl ConflictCollector {
    pub(in crate::storage) fn new(collect_all: bool) -> Self {
        Self {
            collect_all,
            total_conflicts: 0,
            items: Vec::new(),
            first_error: None,
        }
    }

    pub(in crate::storage) fn record(
        &mut self,
        path: impl Into<String>,
        new_target: impl Into<String>,
        existing_target: impl Into<String>,
        message: String,
    ) -> Result<(), PermalinkPutError> {
        if !self.collect_all {
            return Err(PermalinkPutError::Api(ApiError::conflict(message)));
        }
        self.total_conflicts += 1;
        if self.first_error.is_none() {
            self.first_error = Some(message);
        }
        if self.items.len() < MAX_CONFLICT_ITEMS {
            self.items.push(PermalinkConflictItem {
                path: path.into(),
                new_target: new_target.into(),
                existing_target: existing_target.into(),
            });
        }
        Ok(())
    }

    pub(in crate::storage) fn finish(self) -> Result<(), PermalinkPutError> {
        if self.total_conflicts == 0 {
            return Ok(());
        }
        Err(PermalinkPutError::Conflicts(PermalinkConflictError {
            error: self.first_error.unwrap_or_else(|| {
                format!("Phát hiện {} xung đột đường dẫn.", self.total_conflicts)
            }),
            total_conflicts: self.total_conflicts,
            truncated: self.total_conflicts > self.items.len(),
            conflicts: self.items,
        }))
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::storage) fn add_claim(
    all: &mut BTreeMap<String, ProposedClaim>,
    path: String,
    kind: &str,
    id: String,
    locale: &str,
    family: Option<String>,
    generated: bool,
    collector: &mut ConflictCollector,
) -> Result<(), PermalinkPutError> {
    if path.len() > 512
        || !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || path.contains('\\')
        || path.contains("//")
    {
        return Err(PermalinkPutError::Api(ApiError::conflict(format!(
            "permalink_invalid_claim:path={path} target={kind}:{id}:{locale}"
        ))));
    }
    if let Some(previous) = all.get(&path) {
        collector.record(
            &path,
            format!("{kind}:{id}:{locale}"),
            format!("{}:{}:{}", previous.kind, previous.id, previous.locale),
            format!(
                "permalink_collision:path={path} new={kind}:{id}({locale}) existing={}:{}({})",
                previous.kind, previous.id, previous.locale,
            ),
        )?;
        return Ok(());
    }
    all.insert(
        path,
        ProposedClaim {
            kind: kind.to_owned(),
            id,
            locale: locale.to_owned(),
            family,
            generated,
        },
    );
    if all.len() > MAX_CLAIMS {
        return Err(PermalinkPutError::Api(ApiError::conflict(
            "permalink_claim_limit: tối đa 200000 canonical claims.",
        )));
    }
    Ok(())
}
