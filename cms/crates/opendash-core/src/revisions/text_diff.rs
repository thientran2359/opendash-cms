use super::types::{DiffLine, DiffOpKind, TextDiffHunk, TextDiffResult, WordSpan};
use super::{MAX_DIFF_LINES, MAX_DIFF_TOKENS_PER_LINE, MAX_DP_CELLS, MAX_TOTAL_WORD_DP_CELLS};

const CONTEXT_LINES: usize = 3;

#[derive(Debug, Clone)]
enum LineEdit {
    Equal { b_idx: usize, a_idx: usize },
    Removed { b_idx: usize },
    Added { a_idx: usize },
}

fn split_html_blocks(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let normalized_nl = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut with_breaks = String::with_capacity(normalized_nl.len() + 16);
    let bytes = normalized_nl.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(rel_end) = normalized_nl[i..].find('>')
        {
            let tag_end = i + rel_end + 1;
            let tag = &normalized_nl[i..tag_end];
            with_breaks.push_str(tag);
            if is_block_break_tag(tag) && normalized_nl[tag_end..].chars().next() != Some('\n') {
                with_breaks.push('\n');
            }
            i = tag_end;
            continue;
        }
        let ch = normalized_nl[i..].chars().next().unwrap();
        with_breaks.push(ch);
        i += ch.len_utf8();
    }
    with_breaks.lines().map(ToOwned::to_owned).collect()
}

fn is_block_break_tag(tag: &str) -> bool {
    let lower = tag.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "</p>"
            | "</h1>"
            | "</h2>"
            | "</h3>"
            | "</h4>"
            | "</h5>"
            | "</h6>"
            | "</li>"
            | "</div>"
            | "</blockquote>"
            | "<br>"
            | "<br/>"
            | "<br />"
    )
}

fn is_word_scalar(ch: char) -> bool {
    ch.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(&ch) || ch == '_'
}

fn tokenize_unicode_html(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(start, ch)) = chars.peek() {
        if ch == '<' {
            let mut end = start + ch.len_utf8();
            chars.next();
            for (idx, c) in chars.by_ref() {
                end = idx + c.len_utf8();
                if c == '>' {
                    break;
                }
            }
            tokens.push(line[start..end].to_owned());
        } else if ch.is_whitespace() {
            let mut end = start + ch.len_utf8();
            chars.next();
            while let Some(&(idx, c)) = chars.peek() {
                if !c.is_whitespace() || c == '<' {
                    break;
                }
                end = idx + c.len_utf8();
                chars.next();
            }
            tokens.push(line[start..end].to_owned());
        } else if is_word_scalar(ch) {
            let mut end = start + ch.len_utf8();
            chars.next();
            while let Some(&(idx, c)) = chars.peek() {
                if !is_word_scalar(c) {
                    break;
                }
                end = idx + c.len_utf8();
                chars.next();
            }
            tokens.push(line[start..end].to_owned());
        } else {
            let end = start + ch.len_utf8();
            chars.next();
            tokens.push(line[start..end].to_owned());
        }
    }
    tokens
}

fn lcs_table<T: PartialEq>(a: &[T], b: &[T]) -> Vec<DiffOpKind> {
    let n = a.len();
    let m = b.len();
    let cols = m + 1;
    let mut dp = vec![0u32; (n + 1) * cols];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * cols + j] = if a[i] == b[j] {
                1 + dp[(i + 1) * cols + (j + 1)]
            } else {
                dp[(i + 1) * cols + j].max(dp[i * cols + (j + 1)])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(DiffOpKind::Equal);
            i += 1;
            j += 1;
        } else if dp[(i + 1) * cols + j] >= dp[i * cols + (j + 1)] {
            ops.push(DiffOpKind::Removed);
            i += 1;
        } else {
            ops.push(DiffOpKind::Added);
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOpKind::Removed);
        i += 1;
    }
    while j < m {
        ops.push(DiffOpKind::Added);
        j += 1;
    }
    ops
}

fn compute_word_spans(
    before_line: &str,
    after_line: &str,
    budget_left: &mut usize,
    truncated: &mut bool,
) -> Option<Vec<WordSpan>> {
    let tok_a = tokenize_unicode_html(before_line);
    let tok_b = tokenize_unicode_html(after_line);
    if tok_a.len() > MAX_DIFF_TOKENS_PER_LINE || tok_b.len() > MAX_DIFF_TOKENS_PER_LINE {
        *truncated = true;
        return None;
    }
    let prefix = tok_a
        .iter()
        .zip(tok_b.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let max_suffix = (tok_a.len() - prefix).min(tok_b.len() - prefix);
    let suffix = tok_a[prefix..]
        .iter()
        .rev()
        .zip(tok_b[prefix..].iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();

    let mid_a = &tok_a[prefix..tok_a.len() - suffix];
    let mid_b = &tok_b[prefix..tok_b.len() - suffix];
    let cells = mid_a.len().saturating_mul(mid_b.len());
    if cells > *budget_left {
        *truncated = true;
        return None;
    }
    *budget_left -= cells;

    let mut raw_spans = Vec::new();
    for t in &tok_a[..prefix] {
        raw_spans.push((DiffOpKind::Equal, t.as_str()));
    }
    let (mut ia, mut ib) = (0usize, 0usize);
    for op in lcs_table(mid_a, mid_b) {
        match op {
            DiffOpKind::Equal => {
                raw_spans.push((DiffOpKind::Equal, mid_a[ia].as_str()));
                ia += 1;
                ib += 1;
            }
            DiffOpKind::Removed => {
                raw_spans.push((DiffOpKind::Removed, mid_a[ia].as_str()));
                ia += 1;
            }
            DiffOpKind::Added => {
                raw_spans.push((DiffOpKind::Added, mid_b[ib].as_str()));
                ib += 1;
            }
            DiffOpKind::Modified => {}
        }
    }
    for t in &tok_a[tok_a.len() - suffix..] {
        raw_spans.push((DiffOpKind::Equal, t.as_str()));
    }

    let mut merged: Vec<WordSpan> = Vec::new();
    for (kind, text) in raw_spans {
        if text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && last.kind == kind
        {
            last.text.push_str(text);
        } else {
            merged.push(WordSpan {
                kind,
                text: text.to_owned(),
            });
        }
    }
    Some(merged)
}

fn build_diff_lines(
    lines_a: &[String],
    lines_b: &[String],
    edits: &[LineEdit],
    word_budget: &mut usize,
    truncated: &mut bool,
) -> Vec<DiffLine> {
    let mut out = Vec::with_capacity(edits.len());
    let mut idx = 0usize;
    while idx < edits.len() {
        if let LineEdit::Equal { b_idx, a_idx } = edits[idx] {
            out.push(DiffLine {
                kind: DiffOpKind::Equal,
                line_no_before: Some(b_idx + 1),
                line_no_after: Some(a_idx + 1),
                text: lines_b[a_idx].clone(),
                word_spans: Vec::new(),
            });
            idx += 1;
            continue;
        }
        let mut removed = Vec::new();
        let mut added = Vec::new();
        while idx < edits.len() {
            match edits[idx] {
                LineEdit::Removed { b_idx } => {
                    removed.push(b_idx);
                    idx += 1;
                }
                LineEdit::Added { a_idx } => {
                    added.push(a_idx);
                    idx += 1;
                }
                LineEdit::Equal { .. } => break,
            }
        }
        let paired = removed.len().min(added.len());
        for p in 0..paired {
            let b_idx = removed[p];
            let a_idx = added[p];
            if let Some(spans) =
                compute_word_spans(&lines_a[b_idx], &lines_b[a_idx], word_budget, truncated)
            {
                out.push(DiffLine {
                    kind: DiffOpKind::Modified,
                    line_no_before: Some(b_idx + 1),
                    line_no_after: Some(a_idx + 1),
                    text: lines_b[a_idx].clone(),
                    word_spans: spans,
                });
            } else {
                out.push(DiffLine {
                    kind: DiffOpKind::Removed,
                    line_no_before: Some(b_idx + 1),
                    line_no_after: None,
                    text: lines_a[b_idx].clone(),
                    word_spans: Vec::new(),
                });
                out.push(DiffLine {
                    kind: DiffOpKind::Added,
                    line_no_before: None,
                    line_no_after: Some(a_idx + 1),
                    text: lines_b[a_idx].clone(),
                    word_spans: Vec::new(),
                });
            }
        }
        for &b_idx in &removed[paired..] {
            out.push(DiffLine {
                kind: DiffOpKind::Removed,
                line_no_before: Some(b_idx + 1),
                line_no_after: None,
                text: lines_a[b_idx].clone(),
                word_spans: Vec::new(),
            });
        }
        for &a_idx in &added[paired..] {
            out.push(DiffLine {
                kind: DiffOpKind::Added,
                line_no_before: None,
                line_no_after: Some(a_idx + 1),
                text: lines_b[a_idx].clone(),
                word_spans: Vec::new(),
            });
        }
    }
    out
}

fn format_hunks(lines: Vec<DiffLine>) -> (Vec<TextDiffHunk>, usize) {
    let change_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != DiffOpKind::Equal)
        .map(|(idx, _)| idx)
        .collect();
    if change_indices.is_empty() {
        return (Vec::new(), lines.len());
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for ch_idx in change_indices {
        let start = ch_idx.saturating_sub(CONTEXT_LINES);
        let end = (ch_idx + CONTEXT_LINES + 1).min(lines.len());
        if let Some(last) = ranges.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            ranges.push((start, end));
        }
    }

    let mut included = 0usize;
    let mut hunks = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        let slice = lines[start..end].to_vec();
        included += slice.len();
        let before_count = slice.iter().filter(|l| l.line_no_before.is_some()).count();
        let after_count = slice.iter().filter(|l| l.line_no_after.is_some()).count();
        let before_start = slice
            .iter()
            .find_map(|l| l.line_no_before)
            .unwrap_or_else(|| {
                lines[..start]
                    .iter()
                    .rev()
                    .find_map(|l| l.line_no_before)
                    .unwrap_or(0)
                    + 1
            });
        let after_start = slice
            .iter()
            .find_map(|l| l.line_no_after)
            .unwrap_or_else(|| {
                lines[..start]
                    .iter()
                    .rev()
                    .find_map(|l| l.line_no_after)
                    .unwrap_or(0)
                    + 1
            });
        let header = format!("@@ -{before_start},{before_count} +{after_start},{after_count} @@");
        hunks.push(TextDiffHunk {
            header,
            lines: slice,
        });
    }
    let collapsed = lines.len().saturating_sub(included);
    (hunks, collapsed)
}

pub fn compute_line_word_diff(before: &str, after: &str) -> TextDiffResult {
    let lines_a = split_html_blocks(before);
    let lines_b = split_html_blocks(after);
    if lines_a.is_empty() && lines_b.is_empty() {
        return TextDiffResult {
            truncated_diff: false,
            hunks: Vec::new(),
            unchanged_collapsed_count: 0,
        };
    }

    let prefix = lines_a
        .iter()
        .zip(lines_b.iter())
        .take_while(|(x, y)| x == y)
        .count();
    let max_suffix = (lines_a.len() - prefix).min(lines_b.len() - prefix);
    let suffix = lines_a[prefix..]
        .iter()
        .rev()
        .zip(lines_b[prefix..].iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();

    let mid_a = &lines_a[prefix..lines_a.len() - suffix];
    let mid_b = &lines_b[prefix..lines_b.len() - suffix];

    let mut edits = Vec::with_capacity(lines_a.len() + lines_b.len());
    for i in 0..prefix {
        edits.push(LineEdit::Equal { b_idx: i, a_idx: i });
    }

    let mut truncated_diff = false;
    if lines_a.len() > MAX_DIFF_LINES
        || lines_b.len() > MAX_DIFF_LINES
        || mid_a.len().saturating_mul(mid_b.len()) > MAX_DP_CELLS
    {
        truncated_diff = true;
        for b_off in 0..mid_a.len() {
            edits.push(LineEdit::Removed {
                b_idx: prefix + b_off,
            });
        }
        for a_off in 0..mid_b.len() {
            edits.push(LineEdit::Added {
                a_idx: prefix + a_off,
            });
        }
    } else {
        let (mut ia, mut ib) = (0usize, 0usize);
        for op in lcs_table(mid_a, mid_b) {
            match op {
                DiffOpKind::Equal => {
                    edits.push(LineEdit::Equal {
                        b_idx: prefix + ia,
                        a_idx: prefix + ib,
                    });
                    ia += 1;
                    ib += 1;
                }
                DiffOpKind::Removed => {
                    edits.push(LineEdit::Removed { b_idx: prefix + ia });
                    ia += 1;
                }
                DiffOpKind::Added => {
                    edits.push(LineEdit::Added { a_idx: prefix + ib });
                    ib += 1;
                }
                DiffOpKind::Modified => {}
            }
        }
    }

    let tail_a = lines_a.len() - suffix;
    let tail_b = lines_b.len() - suffix;
    for k in 0..suffix {
        edits.push(LineEdit::Equal {
            b_idx: tail_a + k,
            a_idx: tail_b + k,
        });
    }

    let mut word_budget = MAX_TOTAL_WORD_DP_CELLS;
    let diff_lines = build_diff_lines(
        &lines_a,
        &lines_b,
        &edits,
        &mut word_budget,
        &mut truncated_diff,
    );
    let (hunks, unchanged_collapsed_count) = format_hunks(diff_lines);
    TextDiffResult {
        truncated_diff,
        hunks,
        unchanged_collapsed_count,
    }
}
