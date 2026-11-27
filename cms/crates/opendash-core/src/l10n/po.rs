use std::collections::BTreeMap;

use super::{
    Catalog, CatalogEntry, L10nError, MAX_CATALOG_ENTRIES, MAX_ENTRY_BYTES, MAX_MATERIALIZED_BYTES,
    MAX_PO_BYTES, PluralRule, mo::parse_mime_header,
};

#[derive(Default)]
struct RawPoEntry {
    msgctxt: Option<String>,
    msgid: Option<String>,
    msgid_plural: Option<String>,
    msgstr: Option<String>,
    msgstr_plural: BTreeMap<usize, String>,
    fuzzy: bool,
}

#[derive(Clone, Copy)]
enum ActiveField {
    Msgctxt,
    Msgid,
    MsgidPlural,
    Msgstr,
    MsgstrPlural(usize),
}

/// Parses a `.po` text file into a validated `Catalog`.
pub fn parse_po(po_text: &str) -> Result<Catalog, L10nError> {
    if po_text.len() > MAX_PO_BYTES {
        return Err(L10nError::new("File .po vượt quá giới hạn 2 MiB."));
    }
    let clean = po_text.strip_prefix('\u{FEFF}').unwrap_or(po_text);

    let mut raw_entries: Vec<RawPoEntry> = Vec::new();
    let mut current = RawPoEntry::default();
    let mut active: Option<ActiveField> = None;
    let mut has_data = false;

    for raw_line in clean.lines() {
        let line = raw_line.trim_end_matches('\r').trim();
        if line.is_empty() {
            if has_data {
                raw_entries.push(std::mem::take(&mut current));
                has_data = false;
                active = None;
            }
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            if comment.starts_with(',') && comment.contains("fuzzy") {
                current.fuzzy = true;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("msgctxt") {
            let s = parse_quoted_line(rest.trim_start())?;
            current.msgctxt = Some(s);
            active = Some(ActiveField::Msgctxt);
            has_data = true;
        } else if let Some(rest) = line.strip_prefix("msgid_plural") {
            let s = parse_quoted_line(rest.trim_start())?;
            current.msgid_plural = Some(s);
            active = Some(ActiveField::MsgidPlural);
            has_data = true;
        } else if let Some(rest) = line.strip_prefix("msgid") {
            let s = parse_quoted_line(rest.trim_start())?;
            current.msgid = Some(s);
            active = Some(ActiveField::Msgid);
            has_data = true;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let (idx_str, after) = rest
                .split_once(']')
                .ok_or_else(|| L10nError::new("Cú pháp msgstr[k] không hợp lệ."))?;
            let idx: usize = idx_str
                .trim()
                .parse()
                .map_err(|_| L10nError::new("Chỉ số msgstr[k] không hợp lệ."))?;
            let s = parse_quoted_line(after.trim_start())?;
            current.msgstr_plural.insert(idx, s);
            active = Some(ActiveField::MsgstrPlural(idx));
            has_data = true;
        } else if let Some(rest) = line.strip_prefix("msgstr") {
            let s = parse_quoted_line(rest.trim_start())?;
            current.msgstr = Some(s);
            active = Some(ActiveField::Msgstr);
            has_data = true;
        } else if line.starts_with('"') {
            let cont = parse_quoted_line(line)?;
            match active {
                Some(ActiveField::Msgctxt) => {
                    if let Some(v) = current.msgctxt.as_mut() {
                        v.push_str(&cont);
                    }
                }
                Some(ActiveField::Msgid) => {
                    if let Some(v) = current.msgid.as_mut() {
                        v.push_str(&cont);
                    }
                }
                Some(ActiveField::MsgidPlural) => {
                    if let Some(v) = current.msgid_plural.as_mut() {
                        v.push_str(&cont);
                    }
                }
                Some(ActiveField::Msgstr) => {
                    if let Some(v) = current.msgstr.as_mut() {
                        v.push_str(&cont);
                    }
                }
                Some(ActiveField::MsgstrPlural(idx)) => {
                    if let Some(v) = current.msgstr_plural.get_mut(&idx) {
                        v.push_str(&cont);
                    }
                }
                None => return Err(L10nError::new("Chuỗi tiếp nối nằm ngoài entry .po.")),
            }
        } else {
            return Err(L10nError::new(format!("Dòng .po không hợp lệ: {line}")));
        }
    }
    if has_data {
        raw_entries.push(current);
    }

    if raw_entries.len() > MAX_CATALOG_ENTRIES {
        return Err(L10nError::new("Số lượng entry trong .po vượt quá 16.384."));
    }

    let mut plural_rule = PluralRule::default();
    let mut entries: BTreeMap<(Option<String>, String), CatalogEntry> = BTreeMap::new();
    let mut total_bytes: usize = 0;

    for raw in raw_entries {
        let Some(msgid) = raw.msgid else {
            continue;
        };
        if msgid.is_empty() && raw.msgctxt.is_none() {
            if let Some(header) = raw.msgstr.as_deref() {
                plural_rule = parse_mime_header(header)?;
            }
            continue;
        }

        let key = (raw.msgctxt, msgid);
        if entries.contains_key(&key) {
            return Err(L10nError::new(format!(
                "Entry .po bị trùng lặp: '{}'",
                key.1
            )));
        }

        if raw.fuzzy {
            continue;
        }

        let translations = if raw.msgid_plural.is_some() {
            for &k in raw.msgstr_plural.keys() {
                if k >= plural_rule.nplurals {
                    return Err(L10nError::new(format!(
                        "Chỉ số msgstr[{k}] vượt quá nplurals={}.",
                        plural_rule.nplurals
                    )));
                }
            }
            let mut vec = Vec::with_capacity(plural_rule.nplurals);
            for k in 0..plural_rule.nplurals {
                vec.push(raw.msgstr_plural.get(&k).cloned().unwrap_or_default());
            }
            if vec.iter().all(|s| s.is_empty()) {
                continue;
            }
            vec
        } else {
            let val = raw.msgstr.unwrap_or_default();
            if val.is_empty() {
                continue;
            }
            vec![val]
        };

        let entry_bytes = key.1.len()
            + key.0.as_ref().map_or(0, String::len)
            + raw.msgid_plural.as_ref().map_or(0, String::len)
            + translations.iter().map(String::len).sum::<usize>();
        if entry_bytes > MAX_ENTRY_BYTES {
            return Err(L10nError::new("Entry .po vượt quá 64 KiB."));
        }
        total_bytes = total_bytes
            .checked_add(entry_bytes)
            .ok_or_else(|| L10nError::new("Tràn bộ nhớ .po."))?;
        if total_bytes > MAX_MATERIALIZED_BYTES {
            return Err(L10nError::new("Tổng kích thước chuỗi .po vượt quá 8 MiB."));
        }

        entries.insert(
            key,
            CatalogEntry {
                msgid_plural: raw.msgid_plural,
                translations,
            },
        );
    }

    Ok(Catalog::from_parts(plural_rule, entries))
}

/// Compiles a `.po` source string into a GNU `.mo` rev 0.0 Little-Endian binary.
pub fn compile_po_to_mo(po_text: &str) -> Result<Vec<u8>, L10nError> {
    let catalog = parse_po(po_text)?;
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(catalog.len() + 1);

    let header_value = format!(
        "Content-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 8bit\nPlural-Forms: {}\n",
        catalog.plural_rule().raw_header
    );
    pairs.push((Vec::new(), header_value.into_bytes()));

    for ((ctx, msgid), entry) in catalog.entries() {
        let mut orig = Vec::new();
        if let Some(c) = ctx {
            orig.extend_from_slice(c.as_bytes());
            orig.push(0x04);
        }
        orig.extend_from_slice(msgid.as_bytes());
        if let Some(plural) = &entry.msgid_plural {
            orig.push(0x00);
            orig.extend_from_slice(plural.as_bytes());
        }

        let mut trans = Vec::new();
        for (i, t) in entry.translations.iter().enumerate() {
            if i > 0 {
                trans.push(0x00);
            }
            trans.extend_from_slice(t.as_bytes());
        }
        pairs.push((orig, trans));
    }

    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let nstrings = pairs.len();
    let header_len = 28usize;
    let table_len = nstrings * 8;
    let orig_table_off = header_len;
    let trans_table_off = orig_table_off + table_len;
    let mut data_off = trans_table_off + table_len;

    let mut orig_descs = Vec::with_capacity(table_len);
    let mut trans_descs = Vec::with_capacity(table_len);
    let mut data_buf = Vec::new();

    for (orig, _) in &pairs {
        orig_descs.extend_from_slice(&(orig.len() as u32).to_le_bytes());
        orig_descs.extend_from_slice(&(data_off as u32).to_le_bytes());
        data_buf.extend_from_slice(orig);
        data_buf.push(0);
        data_off += orig.len() + 1;
    }

    for (_, trans) in &pairs {
        trans_descs.extend_from_slice(&(trans.len() as u32).to_le_bytes());
        trans_descs.extend_from_slice(&(data_off as u32).to_le_bytes());
        data_buf.extend_from_slice(trans);
        data_buf.push(0);
        data_off += trans.len() + 1;
    }

    let mut out = Vec::with_capacity(data_off);
    out.extend_from_slice(&0x9504_12deu32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(nstrings as u32).to_le_bytes());
    out.extend_from_slice(&(orig_table_off as u32).to_le_bytes());
    out.extend_from_slice(&(trans_table_off as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // hash_table_size = 0
    out.extend_from_slice(&0u32.to_le_bytes()); // hash_table_offset = 0
    out.extend_from_slice(&orig_descs);
    out.extend_from_slice(&trans_descs);
    out.extend_from_slice(&data_buf);
    Ok(out)
}

fn parse_quoted_line(raw: &str) -> Result<String, L10nError> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('"') || !trimmed.ends_with('"') || trimmed.len() < 2 {
        return Err(L10nError::new(
            "Chuỗi .po phải được bọc trong dấu ngoặc kép.",
        ));
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err(L10nError::new("Chuỗi .po kết thúc bằng '\\' không hợp lệ.")),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}
