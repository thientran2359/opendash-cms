use super::*;

pub const TW_PADDING_STEPS: &[i16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64];

pub const TW_MARGIN_STEPS: &[i16] = &[
    -32, -24, -16, -12, -8, -6, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32,
];

pub const TW_GAP_STEPS: &[u16] = &[0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16];

pub(crate) fn px_to_tw_step(px: i16, steps: &[i16], allow_negative: bool) -> Option<i16> {
    if (!allow_negative && px < 0) || px % 4 != 0 {
        return None;
    }
    let step = px / 4;
    steps.contains(&step).then_some(step)
}

pub fn gap_px_to_tw_step(gap_px: u8) -> Option<u16> {
    if gap_px % 4 != 0 {
        return None;
    }
    let step = u16::from(gap_px / 4);
    TW_GAP_STEPS.contains(&step).then_some(step)
}

pub(crate) fn fmt_tw_spacing(bp: &str, prop: &str, axis: &str, step: i16) -> String {
    if step < 0 {
        format!("{bp}-{prop}{axis}-{}", step.unsigned_abs())
    } else {
        format!("{bp}{prop}{axis}-{step}")
    }
}

#[derive(Debug)]
pub enum BuilderError {
    Json(serde_json::Error),
    Invalid(&'static str),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "Invalid Builder JSON: {error}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for BuilderError {}

impl From<serde_json::Error> for BuilderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Target surface context evaluated against each block's `allowed_contexts` allow/deny patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderContextTarget {
    EntryBody {
        content_type: String,
    },
    Pattern,
    ThemePattern,
    LoopItem,
    Template {
        kind: crate::theme::TemplateCandidateKind,
        content_type: Option<String>,
        taxonomy: Option<String>,
        part_key: Option<String>,
    },
}

impl BuilderContextTarget {
    pub fn matches_token(&self, token: &str) -> bool {
        use crate::theme::TemplateCandidateKind as K;

        match token {
            "*" => true,
            "entry_body" => matches!(self, Self::EntryBody { .. }),
            "pattern" => matches!(self, Self::Pattern),
            "theme_pattern" => matches!(self, Self::ThemePattern),
            "template" => matches!(self, Self::Template { .. }),
            "part" => matches!(self, Self::Template { kind: K::Part, .. }),
            "single" => matches!(
                self,
                Self::Template {
                    kind: K::Single | K::SingleCustom | K::Singular,
                    ..
                }
            ),
            "single_custom" => matches!(
                self,
                Self::Template {
                    kind: K::SingleCustom,
                    ..
                }
            ),
            "singular" => matches!(
                self,
                Self::Template {
                    kind: K::Singular,
                    ..
                }
            ),
            "page" => matches!(self, Self::Template { kind: K::Page, .. }),
            "page_layout" => matches!(
                self,
                Self::Template {
                    kind: K::PageLayout,
                    ..
                }
            ),
            "index" => matches!(self, Self::Template { kind: K::Index, .. }),
            "archive" => matches!(
                self,
                Self::Template {
                    kind: K::Archive,
                    ..
                }
            ),
            "taxonomy" => matches!(
                self,
                Self::Template {
                    kind: K::Taxonomy,
                    ..
                }
            ),
            "not_found" => matches!(
                self,
                Self::Template {
                    kind: K::NotFound,
                    ..
                }
            ),
            "search" => matches!(
                self,
                Self::Template {
                    kind: K::Search,
                    ..
                }
            ),
            "loop_item" => matches!(self, Self::LoopItem),
            other => {
                if let Some(cpt) = other.strip_prefix("entry_body:") {
                    matches!(self, Self::EntryBody { content_type } if content_type == cpt)
                } else if let Some(key) = other.strip_prefix("part:") {
                    matches!(
                        self,
                        Self::Template {
                            kind: K::Part,
                            part_key: Some(pk),
                            ..
                        } if pk == key
                    )
                } else if let Some(cpt) = other.strip_prefix("single:") {
                    matches!(
                        self,
                        Self::Template {
                            kind: K::Single | K::SingleCustom | K::Singular,
                            content_type: Some(ct),
                            ..
                        } if ct == cpt
                    )
                } else if let Some(cpt) = other.strip_prefix("archive:") {
                    matches!(
                        self,
                        Self::Template {
                            kind: K::Archive,
                            content_type: Some(ct),
                            ..
                        } if ct == cpt
                    )
                } else if let Some(tax) = other.strip_prefix("taxonomy:") {
                    matches!(
                        self,
                        Self::Template {
                            kind: K::Taxonomy,
                            taxonomy: Some(tx),
                            ..
                        } if tx == tax
                    )
                } else {
                    false
                }
            }
        }
    }

    pub fn allows_contexts(&self, allowed: Option<&[String]>) -> bool {
        let Some(tokens) = allowed else {
            return true;
        };
        let mut has_positive = false;
        let mut matched_positive = false;
        for token in tokens {
            if let Some(neg) = token.strip_prefix('!') {
                if self.matches_token(neg) {
                    return false;
                }
            } else {
                has_positive = true;
                if self.matches_token(token) {
                    matched_positive = true;
                }
            }
        }
        if has_positive { matched_positive } else { true }
    }
}

/// URL policy from ADR-0007: local path or absolute HTTP(S), no credentials.
pub fn valid_button_url(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 2048
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
        || value
            .chars()
            .any(|ch| matches!(ch, '\\' | '<' | '>' | '"' | '\''))
    {
        return false;
    }
    if value.starts_with('/') {
        return !value.starts_with("//")
            && Url::parse(&format!("https://builder.invalid{value}")).is_ok();
    }
    Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
