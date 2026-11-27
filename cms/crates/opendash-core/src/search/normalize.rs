//! Text normalization helpers shared by search indexing, querying, and snippets.

pub const MAX_INDEX_TITLE_BYTES: usize = 1_024;
pub const MAX_INDEX_SUMMARY_BYTES: usize = 2_048;
pub const MAX_INDEX_BODY_BYTES: usize = 128 * 1024;
pub const MAX_INDEX_TERMS_BYTES: usize = 4_096;

/// Folds Vietnamese stroked `đ`/`Đ` into `d`/`D` so SQLite FTS5
/// `unicode61 remove_diacritics 2` matches plain ASCII queries (`duong` ↔ `đường`).
pub fn fold_vietnamese_stroke(input: &str) -> String {
    if !input.contains(['đ', 'Đ']) {
        return input.to_owned();
    }
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            'đ' => out.push('d'),
            'Đ' => out.push('D'),
            _ => out.push(ch),
        }
    }
    out
}

/// Truncates `s` to at most `max_bytes` along a valid UTF-8 scalar boundary.
pub fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> &str {
    let end = s.floor_char_boundary(max_bytes);
    &s[..end]
}

/// Collapses Unicode diacritics (Vietnamese precomposed + common Latin) and folds
/// `đ`/`Đ` to ASCII lowercase for pure-Rust snippet matching without external crates.
pub(crate) fn fold_char_for_snippet(ch: char) -> Option<char> {
    if ('\u{0300}'..='\u{036F}').contains(&ch) || ('\u{1AB0}'..='\u{1AFF}').contains(&ch) {
        return None;
    }
    let folded =
        match ch {
            'a' | 'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ'
            | 'ấ' | 'ẩ' | 'ẫ' | 'ậ' | 'ä' | 'å' | 'ā' | 'À' | 'Á' | 'Ả' | 'Ã' | 'Ạ' | 'Ă' | 'Ằ'
            | 'Ắ' | 'Ẳ' | 'Ẵ' | 'Ặ' | 'Â' | 'Ầ' | 'Ấ' | 'Ẩ' | 'Ẫ' | 'Ậ' | 'Ä' | 'Å' | 'Ā' => {
                'a'
            }
            'c' | 'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => 'c',
            'd' | 'đ' | 'Đ' => 'd',
            'e' | 'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' | 'ë' | 'ē'
            | 'ě' | 'È' | 'É' | 'Ẻ' | 'Ẽ' | 'Ẹ' | 'Ê' | 'Ề' | 'Ế' | 'Ể' | 'Ễ' | 'Ệ' | 'Ë' | 'Ē'
            | 'Ě' => 'e',
            'i' | 'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' | 'î' | 'ï' | 'ī' | 'Ì' | 'Í' | 'Ỉ' | 'Ĩ' | 'Ị'
            | 'Î' | 'Ï' | 'Ī' => 'i',
            'n' | 'ñ' | 'ń' | 'ň' | 'Ñ' | 'Ń' | 'Ň' => 'n',
            'o' | 'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ'
            | 'ớ' | 'ở' | 'ỡ' | 'ợ' | 'ö' | 'ø' | 'ō' | 'Ò' | 'Ó' | 'Ỏ' | 'Õ' | 'Ọ' | 'Ô' | 'Ồ'
            | 'Ố' | 'Ổ' | 'Ỗ' | 'Ộ' | 'Ơ' | 'Ờ' | 'Ớ' | 'Ở' | 'Ỡ' | 'Ợ' | 'Ö' | 'Ø' | 'Ō' => {
                'o'
            }
            's' | 'ś' | 'š' | 'ş' | 'Ś' | 'Š' | 'Ş' => 's',
            'u' | 'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' | 'û' | 'ü'
            | 'ū' | 'Ù' | 'Ú' | 'Ủ' | 'Ũ' | 'Ụ' | 'Ư' | 'Ừ' | 'Ứ' | 'Ử' | 'Ữ' | 'Ự' | 'Û' | 'Ü'
            | 'Ū' => 'u',
            'y' | 'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' | 'ÿ' | 'Ỳ' | 'Ý' | 'Ỷ' | 'Ỹ' | 'Ỵ' | 'Ÿ' => {
                'y'
            }
            'z' | 'ž' | 'ź' | 'ż' | 'Ž' | 'Ź' | 'Ż' => 'z',
            other => {
                let mut lower = other.to_lowercase();
                lower.next().unwrap_or(other)
            }
        };
    Some(folded)
}

pub(crate) fn fold_word_for_snippet(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for ch in word.chars() {
        if let Some(folded) = fold_char_for_snippet(ch) {
            out.push(folded);
        }
    }
    out
}
