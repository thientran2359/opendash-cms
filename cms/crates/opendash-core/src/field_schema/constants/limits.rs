use super::*;

pub const MAX_FIELD_KEY_LEN: usize = 64;

pub const MAX_LABEL_LEN: usize = 100;

pub const MAX_INSTRUCTIONS_LEN: usize = 400;

pub const MAX_PLACEHOLDER_LEN: usize = 120;

pub const MAX_AFFIX_LEN: usize = 32;

pub const DEFAULT_TEXT_MAX_CHARS: usize = 512;

pub const DEFAULT_TEXTAREA_MAX_CHARS: usize = 4_000;

pub const DEFAULT_TEXTAREA_ROWS: u8 = 4;

pub(crate) fn validate_label(label: &str) -> Result<(), &'static str> {
    if label.trim().is_empty()
        || label.chars().count() > MAX_LABEL_LEN
        || label.chars().any(char::is_control)
    {
        return Err("Nhãn field phải không rỗng, tối đa 100 ký tự và không chứa ký tự điều khiển.");
    }
    Ok(())
}
