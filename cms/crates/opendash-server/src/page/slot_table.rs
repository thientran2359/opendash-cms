use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::template_sec::{BLOCK_CHILDREN_NONCE_PREFIX, HtmlBodyScanner, MAX_RENDERED_HTML_BYTES};

#[derive(Debug, Default)]
pub struct HtmlSlotTable {
    entries: BTreeMap<String, String>,
}

impl HtmlSlotTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, nonce: String, html: String) {
        self.entries.insert(nonce, html);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Verifies a container shell contains `nonce` at most once, and when present,
/// that it lands directly inside safe HTML `Body` outside foreign/interactive nesting.
pub fn scan_child_nonce_in_shell(shell: &str, nonce: &str) -> Result<bool, &'static str> {
    let mut matches = shell.match_indices(nonce);
    let Some((offset, _)) = matches.next() else {
        return Ok(false);
    };
    if matches.next().is_some() {
        return Err("Nonce children_html xuất hiện nhiều hơn một lần trong vỏ Container.");
    }
    let mut scanner = HtmlBodyScanner::new();
    for &b in shell[..offset].as_bytes() {
        scanner.consume(b);
    }
    if !scanner.is_safe_container_body() {
        return Err("Nonce children_html không nằm trong thân HTML an toàn của Container.");
    }
    Ok(true)
}

fn is_children_nonce_hex(hex: &str) -> bool {
    hex.len() == 32 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Streams nested container slots together in a single O(N) pass using a bounded frame stack.
pub fn assemble_slot_table(
    root_html: &str,
    table: &mut HtmlSlotTable,
) -> Result<String, &'static str> {
    // Spec A1 MAX_BUILDER_DEPTH (8) + 1 root frame.
    const MAX_ASSEMBLY_FRAMES: usize = 9;
    let nonce_total_len = BLOCK_CHILDREN_NONCE_PREFIX.len() + 32;

    if table.is_empty() {
        if root_html.len() > MAX_RENDERED_HTML_BYTES {
            return Err("HTML render vượt quá giới hạn 512 KiB.");
        }
        return Ok(root_html.to_owned());
    }

    let mut out = String::with_capacity(root_html.len());
    let mut stack: Vec<(Cow<'_, str>, usize)> = Vec::with_capacity(MAX_ASSEMBLY_FRAMES);
    stack.push((Cow::Borrowed(root_html), 0));

    while let Some((src, pos)) = stack.last_mut() {
        let rest = &src[*pos..];
        if let Some(rel) = rest.find(BLOCK_CHILDREN_NONCE_PREFIX) {
            let abs_idx = *pos + rel;
            let chunk = &src[*pos..abs_idx];
            if out.len().saturating_add(chunk.len()) > MAX_RENDERED_HTML_BYTES {
                return Err("Builder template composition exceeds 512 KiB.");
            }
            out.push_str(chunk);

            let maybe_end = abs_idx + nonce_total_len;
            if maybe_end <= src.len() {
                let candidate = &src[abs_idx..maybe_end];
                let hex = &candidate[BLOCK_CHILDREN_NONCE_PREFIX.len()..];
                if is_children_nonce_hex(hex)
                    && let Some(child_html) = table.entries.remove(candidate)
                {
                    *pos = maybe_end;
                    if stack.len() >= MAX_ASSEMBLY_FRAMES {
                        return Err("Độ sâu lồng Container vượt quá giới hạn an toàn.");
                    }
                    stack.push((Cow::Owned(child_html), 0));
                    continue;
                }
            }

            let lit_end = abs_idx + BLOCK_CHILDREN_NONCE_PREFIX.len();
            if out.len().saturating_add(BLOCK_CHILDREN_NONCE_PREFIX.len()) > MAX_RENDERED_HTML_BYTES
            {
                return Err("Builder template composition exceeds 512 KiB.");
            }
            out.push_str(BLOCK_CHILDREN_NONCE_PREFIX);
            *pos = lit_end;
        } else {
            if out.len().saturating_add(rest.len()) > MAX_RENDERED_HTML_BYTES {
                return Err("Builder template composition exceeds 512 KiB.");
            }
            out.push_str(rest);
            stack.pop();
        }
    }
    Ok(out)
}
