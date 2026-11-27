use std::collections::BTreeMap;

use super::{
    Catalog, CatalogEntry, L10nError, MAX_CATALOG_ENTRIES, MAX_ENTRY_BYTES, MAX_MATERIALIZED_BYTES,
    MAX_MO_BYTES, PluralRule,
};

const MO_MAGIC_LE: u32 = 0x9504_12de;
const MO_MAGIC_BE: u32 = 0xde12_0495;

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn read_u32(self, bytes: &[u8], offset: usize) -> Result<u32, L10nError> {
        let slice = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| L10nError::new("File .mo bị cắt cụt (header/table OOB)."))?;
        let arr: [u8; 4] = slice
            .try_into()
            .map_err(|_| L10nError::new("Lỗi đọc u32 trong .mo."))?;
        Ok(match self {
            Endian::Little => u32::from_le_bytes(arr),
            Endian::Big => u32::from_be_bytes(arr),
        })
    }
}

/// Parses a GNU gettext binary `.mo` buffer into a bounded `Catalog`.
pub fn parse_mo(bytes: &[u8]) -> Result<Catalog, L10nError> {
    if bytes.len() > MAX_MO_BYTES {
        return Err(L10nError::new("File .mo vượt quá giới hạn 4 MiB."));
    }
    if bytes.len() < 28 {
        return Err(L10nError::new("File .mo quá nhỏ (< 28 bytes)."));
    }

    let raw_magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let endian = if raw_magic == MO_MAGIC_LE {
        Endian::Little
    } else if raw_magic == MO_MAGIC_BE {
        Endian::Big
    } else {
        return Err(L10nError::new("Magic number file .mo không hợp lệ."));
    };

    let revision = endian.read_u32(bytes, 4)?;
    let major = revision >> 16;
    let minor = revision & 0xffff;
    if major != 0 || minor > 1 {
        return Err(L10nError::new(
            "Phiên bản revision file .mo không được hỗ trợ.",
        ));
    }

    let nstrings = endian.read_u32(bytes, 8)? as usize;
    if nstrings > MAX_CATALOG_ENTRIES {
        return Err(L10nError::new("Số lượng chuỗi trong .mo vượt quá 16.384."));
    }
    let orig_table_offset = endian.read_u32(bytes, 12)? as usize;
    let trans_table_offset = endian.read_u32(bytes, 16)? as usize;

    if minor == 1 && bytes.len() >= 36 {
        // GNU .mo rev 0.1 puts n_sysdep_strings at offset 32 (after hash size + offset).
        let n_sysdep = endian.read_u32(bytes, 32)?;
        if n_sysdep != 0 {
            return Err(L10nError::new(
                "File .mo chứa sysdep_strings không được hỗ trợ.",
            ));
        }
    }

    let table_bytes = nstrings
        .checked_mul(8)
        .ok_or_else(|| L10nError::new("Tràn kích thước bảng .mo."))?;
    let orig_end = orig_table_offset
        .checked_add(table_bytes)
        .ok_or_else(|| L10nError::new("Tràn offset bảng gốc .mo."))?;
    let trans_end = trans_table_offset
        .checked_add(table_bytes)
        .ok_or_else(|| L10nError::new("Tràn offset bảng dịch .mo."))?;
    if orig_end > bytes.len() || trans_end > bytes.len() {
        return Err(L10nError::new("Bảng chuỗi .mo vượt quá kích thước file."));
    }

    let mut total_materialized: usize = 0;
    let mut plural_rule = PluralRule::default();
    let mut entries: BTreeMap<(Option<String>, String), CatalogEntry> = BTreeMap::new();

    for idx in 0..nstrings {
        let orig_desc = orig_table_offset + idx * 8;
        let trans_desc = trans_table_offset + idx * 8;

        let orig_len = endian.read_u32(bytes, orig_desc)? as usize;
        let orig_off = endian.read_u32(bytes, orig_desc + 4)? as usize;
        let trans_len = endian.read_u32(bytes, trans_desc)? as usize;
        let trans_off = endian.read_u32(bytes, trans_desc + 4)? as usize;

        if orig_len > MAX_ENTRY_BYTES || trans_len > MAX_ENTRY_BYTES {
            return Err(L10nError::new("Chuỗi trong .mo vượt quá 64 KiB."));
        }
        total_materialized = total_materialized
            .checked_add(orig_len)
            .and_then(|v| v.checked_add(trans_len))
            .ok_or_else(|| L10nError::new("Tràn bộ nhớ giải mã .mo."))?;
        if total_materialized > MAX_MATERIALIZED_BYTES {
            return Err(L10nError::new(
                "Tổng dung lượng chuỗi giải mã .mo vượt quá 8 MiB.",
            ));
        }

        let orig_slice = read_bounded_slice(bytes, orig_off, orig_len)?;
        let trans_slice = read_bounded_slice(bytes, trans_off, trans_len)?;

        let orig_str = std::str::from_utf8(orig_slice)
            .map_err(|_| L10nError::new("Chuỗi gốc trong .mo không phải UTF-8 hợp lệ."))?;
        let trans_str = std::str::from_utf8(trans_slice)
            .map_err(|_| L10nError::new("Chuỗi dịch trong .mo không phải UTF-8 hợp lệ."))?;

        if orig_str.is_empty() {
            plural_rule = parse_mime_header(trans_str)?;
            continue;
        }

        let (context, msgid_body) = match orig_str.split_once('\x04') {
            Some((ctx, rest)) => (Some(ctx.to_owned()), rest),
            None => (None, orig_str),
        };
        let (msgid, msgid_plural) = match msgid_body.split_once('\x00') {
            Some((s, p)) => (s.to_owned(), Some(p.to_owned())),
            None => (msgid_body.to_owned(), None),
        };
        let translations: Vec<String> = trans_str.split('\x00').map(str::to_owned).collect();
        if translations.iter().all(|t| t.is_empty()) {
            continue;
        }

        entries.insert(
            (context, msgid),
            CatalogEntry {
                msgid_plural,
                translations,
            },
        );
    }

    Ok(Catalog::from_parts(plural_rule, entries))
}

fn read_bounded_slice(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], L10nError> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| L10nError::new("Tràn offset chuỗi .mo."))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| L10nError::new("Offset chuỗi .mo nằm ngoài giới hạn file."))
}

pub(crate) fn parse_mime_header(header_text: &str) -> Result<PluralRule, L10nError> {
    let mut rule = PluralRule::default();
    for line in header_text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("content-type:") {
            if let Some(pos) = lower.find("charset=") {
                let charset = lower[pos + 8..]
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"');
                if !charset.is_empty() && charset != "utf-8" && charset != "utf8" {
                    return Err(L10nError::new("File .mo/.po bắt buộc dùng charset=UTF-8."));
                }
            }
        } else if lower.starts_with("plural-forms:") {
            if let Some((_, val)) = trimmed.split_once(':') {
                rule = PluralRule::parse(val.trim())?;
            }
        }
    }
    Ok(rule)
}
