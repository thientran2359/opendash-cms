use std::collections::HashSet;

use super::{
    CsvRowError, MAX_CSV_IMPORT_BYTES, MAX_CSV_IMPORT_ROWS, ManualRedirectCsvRow,
    ManualRedirectInput, ParsedCsvRow, RedirectStatusCode, RedirectValidationError,
    check_not_self_redirect, normalize_redirect_note, normalize_redirect_source_path,
    normalize_redirect_target_url,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HeaderField {
    SourcePath,
    TargetUrl,
    StatusCode,
    Enabled,
    Note,
    IgnoredExport,
}

fn parse_csv_records(raw: &str) -> Result<Vec<Vec<String>>, RedirectValidationError> {
    let body = raw.strip_prefix('\u{FEFF}').unwrap_or(raw);
    let chars: Vec<char> = body.chars().collect();
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut at_cell_start = true;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    cell.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                // Only delimiter, newline, or EOF may immediately follow a closing quote.
                if i < chars.len() && !matches!(chars[i], ',' | '\r' | '\n') {
                    return Err(RedirectValidationError::CsvMalformedQuote);
                }
                continue;
            }
            cell.push(c);
            i += 1;
            continue;
        }

        if at_cell_start && c == '"' {
            in_quotes = true;
            at_cell_start = false;
            i += 1;
            continue;
        }

        match c {
            '"' => return Err(RedirectValidationError::CsvMalformedQuote),
            ',' => {
                row.push(std::mem::take(&mut cell));
                at_cell_start = true;
                i += 1;
            }
            '\r' => {
                row.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut row));
                at_cell_start = true;
                i += 1;
                if i < chars.len() && chars[i] == '\n' {
                    i += 1;
                }
            }
            '\n' => {
                row.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut row));
                at_cell_start = true;
                i += 1;
            }
            _ => {
                cell.push(c);
                at_cell_start = false;
                i += 1;
            }
        }
    }

    if in_quotes {
        return Err(RedirectValidationError::CsvMalformedQuote);
    }
    if !at_cell_start || !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        records.push(row);
    }

    Ok(records
        .into_iter()
        .filter(|r| !r.iter().all(|col| col.trim().is_empty()))
        .collect())
}

fn map_header_alias(col: &str) -> HeaderField {
    match col.trim().to_ascii_lowercase().as_str() {
        "source_path" | "source" | "from" | "old_url" | "old_path" => HeaderField::SourcePath,
        "target_url" | "target" | "to" | "new_url" | "destination" => HeaderField::TargetUrl,
        "status_code" | "status" | "code" | "http_status" => HeaderField::StatusCode,
        "enabled" | "active" | "is_active" => HeaderField::Enabled,
        "note" | "notes" | "comment" | "description" => HeaderField::Note,
        _ => HeaderField::IgnoredExport,
    }
}

fn looks_like_header_row(row: &[String]) -> bool {
    if row.is_empty() {
        return false;
    }
    let mut non_empty = 0usize;
    let mut matched_known = 0usize;
    for cell in row {
        let t = cell.trim();
        if t.is_empty() {
            continue;
        }
        non_empty += 1;
        let lower = t.to_ascii_lowercase();
        if lower.starts_with('/')
            || lower.starts_with("http://")
            || lower.starts_with("https://")
            || !lower
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return false;
        }
        if matches!(
            lower.as_str(),
            "source_path"
                | "source"
                | "from"
                | "old_url"
                | "old_path"
                | "target_url"
                | "target"
                | "to"
                | "new_url"
                | "destination"
                | "status_code"
                | "status"
                | "code"
                | "http_status"
                | "enabled"
                | "active"
                | "is_active"
                | "note"
                | "notes"
                | "comment"
                | "description"
                | "hit_count"
                | "hits"
                | "last_hit_at"
                | "version"
                | "created_at"
                | "updated_at"
        ) {
            matched_known += 1;
        }
    }
    if non_empty == 1 {
        matched_known == 1
    } else {
        matched_known >= 2
    }
}

fn parse_status_cell(raw: &str) -> Result<RedirectStatusCode, CsvRowError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(RedirectStatusCode::MovedPermanently);
    }
    match trimmed {
        "301" => Ok(RedirectStatusCode::MovedPermanently),
        "302" => Ok(RedirectStatusCode::Found),
        _ => Err(CsvRowError::InvalidStatusValue),
    }
}

fn parse_enabled_cell(raw: &str) -> Result<bool, CsvRowError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(true);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(CsvRowError::InvalidEnabledValue),
    }
}

pub fn parse_redirects_csv(
    raw: &str,
) -> Result<Vec<Result<ParsedCsvRow, CsvRowError>>, RedirectValidationError> {
    if raw.len() > MAX_CSV_IMPORT_BYTES {
        return Err(RedirectValidationError::CsvTooLarge);
    }
    let records = parse_csv_records(raw)?;
    if records.is_empty() {
        return Err(RedirectValidationError::CsvEmpty);
    }

    let (header_map, data_rows): (Option<Vec<HeaderField>>, &[Vec<String>]) =
        if looks_like_header_row(&records[0]) {
            let mut seen = HashSet::new();
            let mut mapped = Vec::with_capacity(records[0].len());
            for col in &records[0] {
                let field = map_header_alias(col);
                if field != HeaderField::IgnoredExport && !seen.insert(field) {
                    return Err(RedirectValidationError::CsvDuplicateMappedHeaders);
                }
                mapped.push(field);
            }
            if !seen.contains(&HeaderField::SourcePath) || !seen.contains(&HeaderField::TargetUrl) {
                return Err(RedirectValidationError::CsvMissingRequiredHeaders);
            }
            (Some(mapped), &records[1..])
        } else {
            (None, &records[..])
        };

    if data_rows.is_empty() {
        return Err(RedirectValidationError::CsvEmpty);
    }
    if data_rows.len() > MAX_CSV_IMPORT_ROWS {
        return Err(RedirectValidationError::CsvTooManyRows);
    }

    let mut parsed = Vec::with_capacity(data_rows.len());
    for (idx, cols) in data_rows.iter().enumerate() {
        let row_number = idx + 1;
        let row_result = parse_single_csv_row(row_number, cols, header_map.as_deref());
        parsed.push(row_result);
    }
    Ok(parsed)
}

fn parse_single_csv_row(
    row_number: usize,
    cols: &[String],
    header_map: Option<&[HeaderField]>,
) -> Result<ParsedCsvRow, CsvRowError> {
    let (src_raw, tgt_raw, status_raw, enabled_raw, note_raw) = if let Some(map) = header_map {
        if cols.len() > map.len() {
            return Err(CsvRowError::InvalidColumnCount);
        }
        let mut src = "";
        let mut tgt = "";
        let mut status = "";
        let mut enabled = "";
        let mut note = "";
        for (idx, field) in map.iter().enumerate() {
            let val = cols.get(idx).map(String::as_str).unwrap_or("");
            match field {
                HeaderField::SourcePath => src = val,
                HeaderField::TargetUrl => tgt = val,
                HeaderField::StatusCode => status = val,
                HeaderField::Enabled => enabled = val,
                HeaderField::Note => note = val,
                HeaderField::IgnoredExport => {}
            }
        }
        (src, tgt, status, enabled, note)
    } else {
        if !(2..=5).contains(&cols.len()) {
            return Err(CsvRowError::InvalidColumnCount);
        }
        (
            cols[0].as_str(),
            cols[1].as_str(),
            cols.get(2).map(String::as_str).unwrap_or(""),
            cols.get(3).map(String::as_str).unwrap_or(""),
            cols.get(4).map(String::as_str).unwrap_or(""),
        )
    };

    let source_path = normalize_redirect_source_path(src_raw)?;
    let target_url = normalize_redirect_target_url(tgt_raw)?;
    check_not_self_redirect(&source_path, &target_url, None)?;
    let status_code = parse_status_cell(status_raw)?;
    let enabled = parse_enabled_cell(enabled_raw)?;
    let note = normalize_redirect_note(if note_raw.trim().is_empty() {
        None
    } else {
        Some(note_raw)
    })?;
    Ok(ParsedCsvRow {
        row_number,
        input: ManualRedirectInput {
            source_path,
            target_url,
            status_code,
            enabled,
            note,
        },
    })
}

fn escape_csv_formula_cell(val: &str) -> String {
    let trimmed_start = val.trim_start();
    let needs_prefix = val.starts_with(['\t', '\r', '\n'])
        || trimmed_start.starts_with(['=', '+', '-', '@', '\t', '\r', '\n']);
    let safe = if needs_prefix {
        format!("'{val}")
    } else {
        val.to_owned()
    };
    if safe.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", safe.replace('"', "\"\""))
    } else {
        safe
    }
}

pub fn format_redirects_csv(rows: &[ManualRedirectCsvRow]) -> String {
    let mut out = String::from(
        "\u{FEFF}source_path,target_url,status_code,enabled,note,hit_count,last_hit_at\r\n",
    );
    for row in rows {
        out.push_str(&escape_csv_formula_cell(&row.source_path));
        out.push(',');
        out.push_str(&escape_csv_formula_cell(&row.target_url));
        out.push(',');
        out.push_str(&row.status_code.to_string());
        out.push(',');
        out.push_str(if row.enabled { "true" } else { "false" });
        out.push(',');
        out.push_str(&escape_csv_formula_cell(&row.note));
        out.push(',');
        out.push_str(&row.hit_count.to_string());
        out.push(',');
        out.push_str(&escape_csv_formula_cell(
            row.last_hit_at.as_deref().unwrap_or(""),
        ));
        out.push_str("\r\n");
    }
    out
}
