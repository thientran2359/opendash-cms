use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use opendash_core::l10n::{PotDocument, compile_po_to_mo, parse_po, validate_placeholders};

#[derive(Default)]
struct ExtractedItem {
    msgctxt: Option<String>,
    msgid: String,
    msgid_plural: Option<String>,
    refs: BTreeSet<String>,
}

/// Runs `--l10n-extract <dir> --out <file.pot> [--domain <name>]` using a zero-dependency lexical scanner.
pub fn run_extract_cli(
    source_dir: &Path,
    out_pot: &Path,
    domain: Option<&str>,
) -> Result<usize, String> {
    if !source_dir.is_dir() {
        return Err(format!(
            "Thư mục nguồn không tồn tại: {}",
            source_dir.display()
        ));
    }
    let mut items: BTreeMap<(Option<String>, String), ExtractedItem> = BTreeMap::new();
    visit_source_tree(source_dir, source_dir, domain, &mut items)?;

    let mut out = String::from(
        "# OpenDash Translation Template.\n\
         msgid \"\"\n\
         msgstr \"\"\n\
         \"Content-Type: text/plain; charset=UTF-8\\n\"\n\
         \"Content-Transfer-Encoding: 8bit\\n\"\n\
         \"Plural-Forms: nplurals=2; plural=(n != 1);\\n\"\n\n",
    );
    for item in items.values() {
        if !item.refs.is_empty() {
            let refs: Vec<&str> = item.refs.iter().take(5).map(String::as_str).collect();
            out.push_str(&format!("#: {}\n", refs.join(" ")));
        }
        if let Some(ctx) = &item.msgctxt {
            out.push_str(&format!("msgctxt {}\n", po_quote(ctx)));
        }
        out.push_str(&format!("msgid {}\n", po_quote(&item.msgid)));
        if let Some(plural) = &item.msgid_plural {
            out.push_str(&format!("msgid_plural {}\n", po_quote(plural)));
            out.push_str("msgstr[0] \"\"\nmsgstr[1] \"\"\n\n");
        } else {
            out.push_str("msgstr \"\"\n\n");
        }
    }
    if let Some(parent) = out_pot.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(out_pot, out).map_err(|e| e.to_string())?;
    Ok(items.len())
}

/// Runs `--l10n-compile <file.po> --out <file.mo> [--pot <file.pot>]`.
pub fn run_compile_cli(
    po_path: &Path,
    out_mo: &Path,
    pot_path: Option<&Path>,
) -> Result<usize, String> {
    let po_text = fs::read_to_string(po_path).map_err(|e| format!("Không đọc được .po: {e}"))?;
    let catalog = parse_po(&po_text).map_err(|e| e.message)?;
    for ((_, msgid), entry) in catalog.entries() {
        if let Some(plural) = &entry.msgid_plural {
            for t in &entry.translations {
                if !t.is_empty() {
                    validate_placeholders(plural, t).map_err(|e| e.message)?;
                }
            }
        } else if let Some(t) = entry.translations.first()
            && !t.is_empty()
        {
            validate_placeholders(msgid, t).map_err(|e| e.message)?;
        }
    }
    if let Some(pot_file) = pot_path {
        let pot_text =
            fs::read_to_string(pot_file).map_err(|e| format!("Không đọc được .pot: {e}"))?;
        let pot = PotDocument::parse(&pot_text).map_err(|e| e.message)?;
        let _coverage = catalog.coverage_against_pot(&pot);
    }
    let mo_bytes = compile_po_to_mo(&po_text).map_err(|e| e.message)?;
    if let Some(parent) = out_mo.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(out_mo, mo_bytes).map_err(|e| e.to_string())?;
    Ok(catalog.len())
}

fn visit_source_tree(
    root: &Path,
    dir: &Path,
    domain: Option<&str>,
    items: &mut BTreeMap<(Option<String>, String), ExtractedItem>,
) -> Result<(), String> {
    let rd = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in rd {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if matches!(
            name_str.as_ref(),
            "target" | "node_modules" | ".git" | "dist"
        ) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            visit_source_tree(root, &path, domain, items)?;
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "rs" | "ts" | "tsx" | "js" | "html") {
            continue;
        }
        let Ok(src) = fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scan_source_calls(&src, &rel, domain, items)?;
    }
    Ok(())
}

fn scan_source_calls(
    src: &str,
    rel: &str,
    domain_filter: Option<&str>,
    items: &mut BTreeMap<(Option<String>, String), ExtractedItem>,
) -> Result<(), String> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut line = 1usize;
    let patterns: &[(&str, u8)] = &[
        ("t!(", 1),
        ("tx!(", 2),
        ("tn!(", 3),
        ("__(", 1),
        ("_x(", 2),
        ("_n(", 3),
        ("_nx(", 4),
    ];

    while i < bytes.len() {
        if bytes[i] == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        let prev_ok = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if prev_ok {
            let mut matched = None;
            for &(pat, kind) in patterns {
                if bytes[i..].starts_with(pat.as_bytes()) {
                    matched = Some((pat, kind));
                    break;
                }
            }
            if let Some((pat, kind)) = matched {
                let mut cursor = i + pat.len();
                let args = parse_call_string_args(src, &mut cursor);
                record_extracted_args(kind, &args, rel, line, domain_filter, items)?;
                i = cursor;
                continue;
            }
        }
        i += 1;
    }
    Ok(())
}

fn parse_call_string_args(src: &str, cursor: &mut usize) -> Vec<Option<String>> {
    let bytes = src.as_bytes();
    let mut args = Vec::new();
    let mut depth = 1usize;
    let mut cur_str: Option<String> = None;
    let mut expect_arg = true;

    while *cursor < bytes.len() && depth > 0 {
        let b = bytes[*cursor];
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            *cursor += 1;
            continue;
        }
        if b == b',' && depth == 1 {
            args.push(cur_str.take());
            expect_arg = true;
            *cursor += 1;
            continue;
        }
        if b == b')' {
            depth -= 1;
            if depth == 0 {
                if !expect_arg || cur_str.is_some() {
                    args.push(cur_str.take());
                }
                *cursor += 1;
                break;
            }
            *cursor += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            expect_arg = false;
            *cursor += 1;
            continue;
        }
        if (b == b'"' || b == b'\'') && depth == 1 && expect_arg {
            let q = b;
            *cursor += 1;
            let mut s = String::new();
            while *cursor < bytes.len() {
                let ch = bytes[*cursor];
                if ch == b'\\' && *cursor + 1 < bytes.len() {
                    let esc = bytes[*cursor + 1];
                    match esc {
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'r' => s.push('\r'),
                        b'"' => s.push('"'),
                        b'\'' => s.push('\''),
                        b'\\' => s.push('\\'),
                        other => s.push(other as char),
                    }
                    *cursor += 2;
                    continue;
                }
                if ch == q {
                    *cursor += 1;
                    break;
                }
                if let Some(ch_str) = src[*cursor..].chars().next() {
                    s.push(ch_str);
                    *cursor += ch_str.len_utf8();
                } else {
                    break;
                }
            }
            cur_str = Some(s);
            expect_arg = false;
            continue;
        }
        expect_arg = false;
        *cursor += 1;
    }
    args
}

fn record_extracted_args(
    kind: u8,
    args: &[Option<String>],
    rel: &str,
    line: usize,
    domain_filter: Option<&str>,
    items: &mut BTreeMap<(Option<String>, String), ExtractedItem>,
) -> Result<(), String> {
    let (msgid, msgid_plural, msgctxt, domain) = match kind {
        1 => (
            args.first().and_then(|a| a.clone()),
            None,
            None,
            args.get(1).and_then(|a| a.clone()),
        ),
        2 => (
            args.first().and_then(|a| a.clone()),
            None,
            args.get(1).and_then(|a| a.clone()),
            args.get(2).and_then(|a| a.clone()),
        ),
        3 => (
            args.first().and_then(|a| a.clone()),
            args.get(1).and_then(|a| a.clone()),
            None,
            args.get(3).and_then(|a| a.clone()),
        ),
        4 => (
            args.first().and_then(|a| a.clone()),
            args.get(1).and_then(|a| a.clone()),
            args.get(3).and_then(|a| a.clone()),
            args.get(4).and_then(|a| a.clone()),
        ),
        _ => return Ok(()),
    };
    let Some(msgid) = msgid.filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if let Some(filter) = domain_filter
        && domain.as_deref().unwrap_or("opendash") != filter
    {
        return Ok(());
    }
    let key = (msgctxt.clone(), msgid.clone());
    let entry = items.entry(key).or_insert_with(|| ExtractedItem {
        msgctxt,
        msgid,
        msgid_plural: msgid_plural.clone(),
        refs: BTreeSet::new(),
    });
    if let (Some(existing), Some(new_p)) = (&entry.msgid_plural, &msgid_plural)
        && existing != new_p
    {
        return Err(format!(
            "Xung đột msgid_plural cho '{}' tại {rel}:{line}: '{}' vs '{}'",
            entry.msgid, existing, new_p
        ));
    }
    if entry.msgid_plural.is_none() {
        entry.msgid_plural = msgid_plural;
    }
    entry.refs.insert(format!("{rel}:{line}"));
    Ok(())
}

fn po_quote(str_val: &str) -> String {
    let escaped = str_val
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}
