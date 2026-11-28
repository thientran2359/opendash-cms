//! Structured excerpt builder producing auto-escape-safe `SnippetSegment` slices.

use serde::{Deserialize, Serialize};

use super::normalize::fold_word_for_snippet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnippetSegment {
    pub text: String,
    pub matched: bool,
}

#[derive(Debug, Clone)]
struct WordSpan {
    byte_start: usize,
    byte_end: usize,
    char_start: usize,
    char_end: usize,
    matched: bool,
}

/// Generates a bounded window (`~max_chars`) from `source_text` preserving
/// original casing and Vietnamese diacritics while marking matched words with
/// `matched: true` for `<mark>` rendering.
pub fn build_structured_snippet(
    source_text: &str,
    match_tokens: &[String],
    max_chars: usize,
) -> Vec<SnippetSegment> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let folded_tokens: Vec<String> = match_tokens
        .iter()
        .map(|tok| fold_word_for_snippet(tok))
        .filter(|tok| !tok.is_empty())
        .collect();

    let spans = collect_word_spans(trimmed, &folded_tokens);
    let total_chars = trimmed.chars().count();

    let first_match = spans.iter().find(|span| span.matched);
    let anchor_char = first_match.map(|span| span.char_start).unwrap_or(0);

    let lead_chars = max_chars / 3;
    let raw_win_start_char = if anchor_char > lead_chars {
        anchor_char - lead_chars
    } else {
        0
    };

    let (win_start_byte, win_start_char) = if raw_win_start_char == 0 {
        (0, 0)
    } else if let Some(span) = spans
        .iter()
        .find(|span| span.char_start >= raw_win_start_char && span.char_start <= anchor_char)
    {
        (span.byte_start, span.char_start)
    } else {
        first_match
            .map(|span| (span.byte_start, span.char_start))
            .unwrap_or((0, 0))
    };

    let target_end_char = win_start_char.saturating_add(max_chars).min(total_chars);
    let mut win_end_byte = if target_end_char >= total_chars {
        trimmed.len()
    } else {
        let byte_limit = char_idx_to_byte(trimmed, target_end_char);
        spans
            .iter()
            .rev()
            .find(|span| span.byte_start >= win_start_byte && span.byte_end <= byte_limit)
            .map(|span| span.byte_end)
            .unwrap_or_else(|| first_span_end_or_limit(trimmed, &spans, win_start_byte, byte_limit))
    };

    // Never leave only half of an isolated first match outside the window.
    if let Some(m) = first_match
        && m.byte_start >= win_start_byte
        && m.byte_end > win_end_byte
        && win_start_byte == m.byte_start
    {
        win_end_byte = trimmed
            .char_indices()
            .take_while(|(idx, _)| *idx < m.byte_end)
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or(trimmed.len());
    }

    let leading_clipped = win_start_byte > 0;
    let trailing_clipped = win_end_byte < trimmed.len();

    let mut segments = Vec::new();
    if leading_clipped {
        push_segment(&mut segments, "…".to_owned(), false);
    }

    let mut cursor = win_start_byte;
    for span in &spans {
        if !span.matched || span.byte_end <= win_start_byte || span.byte_start >= win_end_byte {
            continue;
        }
        let seg_start = span.byte_start.max(cursor);
        let seg_end = span.byte_end.min(win_end_byte);
        if seg_start > cursor {
            push_segment(&mut segments, trimmed[cursor..seg_start].to_owned(), false);
        }
        if seg_end > seg_start {
            push_segment(&mut segments, trimmed[seg_start..seg_end].to_owned(), true);
        }
        cursor = seg_end;
    }
    if cursor < win_end_byte {
        push_segment(
            &mut segments,
            trimmed[cursor..win_end_byte].to_owned(),
            false,
        );
    }
    if trailing_clipped {
        push_segment(&mut segments, "…".to_owned(), false);
    }

    segments
}

fn collect_word_spans(text: &str, folded_tokens: &[String]) -> Vec<WordSpan> {
    let mut spans = Vec::new();
    let mut start: Option<(usize, usize)> = None;
    let mut char_idx = 0usize;

    for (byte_idx, ch) in text.char_indices() {
        let in_word = ch.is_alphanumeric()
            || ('\u{0300}'..='\u{036F}').contains(&ch)
            || ('\u{1AB0}'..='\u{1AFF}').contains(&ch);
        if in_word {
            if start.is_none() {
                start = Some((byte_idx, char_idx));
            }
        } else if let Some((byte_start, char_start)) = start.take() {
            let word = &text[byte_start..byte_idx];
            spans.push(WordSpan {
                byte_start,
                byte_end: byte_idx,
                char_start,
                char_end: char_idx,
                matched: is_word_match(word, folded_tokens),
            });
        }
        char_idx += 1;
    }
    if let Some((byte_start, char_start)) = start {
        let word = &text[byte_start..];
        spans.push(WordSpan {
            byte_start,
            byte_end: text.len(),
            char_start,
            char_end: char_idx,
            matched: is_word_match(word, folded_tokens),
        });
    }
    spans
}

fn is_word_match(raw_word: &str, folded_tokens: &[String]) -> bool {
    if folded_tokens.is_empty() {
        return false;
    }
    let word = fold_word_for_snippet(raw_word);
    let last_idx = folded_tokens.len() - 1;
    folded_tokens.iter().enumerate().any(|(idx, tok)| {
        if word == *tok {
            true
        } else {
            idx == last_idx && tok.chars().count() >= 2 && word.starts_with(tok.as_str())
        }
    })
}

fn char_idx_to_byte(s: &str, target_char: usize) -> usize {
    s.char_indices()
        .nth(target_char)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

fn first_span_end_or_limit(
    trimmed: &str,
    spans: &[WordSpan],
    win_start_byte: usize,
    byte_limit: usize,
) -> usize {
    if let Some(span) = spans
        .iter()
        .find(|span| span.byte_start == win_start_byte && span.char_end > span.char_start)
    {
        span.byte_end.max(byte_limit)
    } else {
        trimmed.floor_char_boundary(byte_limit.min(trimmed.len()))
    }
}

fn push_segment(out: &mut Vec<SnippetSegment>, text: String, matched: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.matched == matched
    {
        last.text.push_str(&text);
    } else {
        out.push(SnippetSegment { text, matched });
    }
}
