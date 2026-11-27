use super::*;

impl PermalinkPutError {
    pub(crate) fn into_api_error(self) -> ApiError {
        match self {
            Self::Api(err) => err,
            Self::Conflicts(c) => ApiError::conflict(c.error),
        }
    }

    #[cfg(test)]
    pub fn status(&self) -> axum::http::StatusCode {
        match self {
            Self::Api(err) => err.0,
            Self::Conflicts(_) => axum::http::StatusCode::CONFLICT,
        }
    }
}

#[cfg(test)]
impl PermalinkSettings {
    pub(in crate::storage::permalinks) fn input(&self) -> PermalinkSettingsInput {
        PermalinkSettingsInput {
            post_structure: self.post_structure.clone(),
            content_type_bases: self.content_type_bases.clone(),
            taxonomy_bases: self.taxonomy_bases.clone(),
        }
    }
}
