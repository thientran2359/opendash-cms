//! FTS5 query sanitizer neutralizing syntax operators while preserving
//! clean display copy and Vietnamese/prefix matching semantics.

use std::fmt;

use super::normalize::fold_vietnamese_stroke;

pub const MAX_SEARCH_QUERY_CHARS: usize = 200;
pub const MAX_SEARCH_QUERY_BYTES: usize = 512;
pub const MAX_SEARCH_TOKENS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchQueryError {
    TooLong,
}

impl fmt::Display for SearchQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(
                f,
                "Từ khóa tìm kiếm vượt quá giới hạn {MAX_SEARCH_QUERY_CHARS} ký tự hoặc {MAX_SEARCH_QUERY_BYTES} bytes."
            ),
        }
    }
}

impl std::error::Error for SearchQueryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedSearchQuery {
    pub display_query: String,
    pub is_empty: bool,
    pub tokens: Vec<String>,
    pub fts_match_expr: Option<String>,
}

/// Validates bounds and translates arbitrary user search text into a safe
/// double-quoted FTS5 `MATCH` expression. Punctuation-only queries yield
/// `is_empty = false` and `fts_match_expr = None` so routes return `200 OK`
/// with empty hits instead of an FTS5 syntax error.
pub fn sanitize_search_query(raw: &str) -> Result<SanitizedSearchQuery, SearchQueryError> {
    let trimmed = raw.trim();
    if trimmed.len() > MAX_SEARCH_QUERY_BYTES || trimmed.chars().count() > MAX_SEARCH_QUERY_CHARS {
        return Err(SearchQueryError::TooLong);
    }

    let cleaned: String = trimmed
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let display_query = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if display_query.is_empty() {
        return Ok(SanitizedSearchQuery {
            display_query: String::new(),
            is_empty: true,
            tokens: Vec::new(),
            fts_match_expr: None,
        });
    }

    let folded = fold_vietnamese_stroke(&display_query);
    let mut tokens = Vec::new();
    let mut start_idx: Option<usize> = None;

    for (idx, ch) in folded.char_indices() {
        let is_token_char = ch.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&ch);
        if is_token_char {
            if start_idx.is_none() {
                start_idx = Some(idx);
            }
        } else if let Some(start) = start_idx.take() {
            push_token(&folded[start..idx], &mut tokens);
            if tokens.len() >= MAX_SEARCH_TOKENS {
                break;
            }
        }
    }
    if tokens.len() < MAX_SEARCH_TOKENS
        && let Some(start) = start_idx
    {
        push_token(&folded[start..], &mut tokens);
    }

    if tokens.is_empty() {
        return Ok(SanitizedSearchQuery {
            display_query,
            is_empty: false,
            tokens,
            fts_match_expr: None,
        });
    }

    let last_idx = tokens.len() - 1;
    let quoted: Vec<String> = tokens
        .iter()
        .enumerate()
        .map(|(idx, tok)| {
            if idx == last_idx && tok.chars().count() >= 2 {
                format!("\"{tok}\"*")
            } else {
                format!("\"{tok}\"")
            }
        })
        .collect();

    Ok(SanitizedSearchQuery {
        display_query,
        is_empty: false,
        tokens,
        fts_match_expr: Some(quoted.join(" ")),
    })
}

fn push_token(raw_token: &str, out: &mut Vec<String>) {
    let lower = raw_token.to_lowercase();
    if !lower.is_empty() {
        out.push(lower);
    }
}
