mod csv;
mod path;

#[cfg(test)]
mod tests;

pub use csv::*;
pub use path::*;

use serde::{Deserialize, Serialize};

pub const MAX_REDIRECT_SOURCE_PATH_BYTES: usize = 512;
pub const MAX_REDIRECT_TARGET_URL_BYTES: usize = 2048;
pub const MAX_REDIRECT_NOTE_CHARS: usize = 256;
pub const MAX_MANUAL_REDIRECTS_TOTAL: usize = 20_000;
pub const MAX_CSV_IMPORT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CSV_IMPORT_ROWS: usize = 5_000;
pub const MAX_CSV_PREVIEW_ROWS: usize = 500;
pub const MAX_NOT_FOUND_LOG_ROWS: usize = 5_000;
pub const MAX_NOT_FOUND_PATH_BYTES: usize = 512;
pub const MAX_NOT_FOUND_REFERRER_BYTES: usize = 512;
pub const MAX_NOT_FOUND_UA_BYTES: usize = 256;
pub const MAX_REDIRECT_CHAIN_HOPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectValidationError {
    EmptySourcePath,
    SourcePathTooLong,
    SourcePathInvalidFormat,
    SourcePathReservedRoot,
    SourcePathUnsafeEncoding,
    EmptyTargetUrl,
    TargetUrlTooLong,
    TargetUrlInvalid,
    TargetUrlUnsafeProtocolRelative,
    SelfRedirect,
    InvalidStatusCode,
    NoteTooLong,
    NoteControlChars,
    CsvEmpty,
    CsvTooLarge,
    CsvTooManyRows,
    CsvMalformedQuote,
    CsvMissingRequiredHeaders,
    CsvDuplicateMappedHeaders,
}

impl RedirectValidationError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptySourcePath => "redirects.empty_source_path",
            Self::SourcePathTooLong => "redirects.source_path_too_long",
            Self::SourcePathInvalidFormat => "redirects.source_path_invalid_format",
            Self::SourcePathReservedRoot => "redirects.source_path_reserved_root",
            Self::SourcePathUnsafeEncoding => "redirects.source_path_unsafe_encoding",
            Self::EmptyTargetUrl => "redirects.empty_target_url",
            Self::TargetUrlTooLong => "redirects.target_url_too_long",
            Self::TargetUrlInvalid => "redirects.target_url_invalid",
            Self::TargetUrlUnsafeProtocolRelative => {
                "redirects.target_url_unsafe_protocol_relative"
            }
            Self::SelfRedirect => "redirects.self_redirect",
            Self::InvalidStatusCode => "redirects.invalid_status_code",
            Self::NoteTooLong => "redirects.note_too_long",
            Self::NoteControlChars => "redirects.note_control_chars",
            Self::CsvEmpty => "redirects.csv_empty",
            Self::CsvTooLarge => "redirects.csv_too_large",
            Self::CsvTooManyRows => "redirects.csv_too_many_rows",
            Self::CsvMalformedQuote => "redirects.csv_malformed_quote",
            Self::CsvMissingRequiredHeaders => "redirects.csv_missing_required_headers",
            Self::CsvDuplicateMappedHeaders => "redirects.csv_duplicate_mapped_headers",
        }
    }

    pub const fn canonical_vi_message(self) -> &'static str {
        match self {
            Self::EmptySourcePath => "Đường dẫn nguồn không được để trống.",
            Self::SourcePathTooLong => "Đường dẫn nguồn vượt quá giới hạn 512 byte.",
            Self::SourcePathInvalidFormat => {
                "Đường dẫn nguồn phải bắt đầu bằng '/', không phải '/', và không chứa khoảng trắng, '?', '#', '\\' hoặc đoạn rỗng/'.'/'..'."
            }
            Self::SourcePathReservedRoot => {
                "Đường dẫn nguồn trùng với tiền tố hệ thống được bảo vệ."
            }
            Self::SourcePathUnsafeEncoding => {
                "Đường dẫn nguồn chứa mã hóa phần trăm không hợp lệ hoặc ký tự ASCII bị cấm."
            }
            Self::EmptyTargetUrl => "URL đích không được để trống.",
            Self::TargetUrlTooLong => "URL đích vượt quá giới hạn 2048 byte.",
            Self::TargetUrlInvalid => {
                "URL đích phải là đường dẫn nội bộ bắt đầu bằng '/' hoặc URL http(s):// hợp lệ không chứa thông tin đăng nhập."
            }
            Self::TargetUrlUnsafeProtocolRelative => {
                "URL đích không được bắt đầu bằng '//' hoặc '/\\'."
            }
            Self::SelfRedirect => "Đường dẫn nguồn và URL đích không được trỏ về chính nó.",
            Self::InvalidStatusCode => "Mã trạng thái chuyển hướng chỉ hỗ trợ 301 hoặc 302.",
            Self::NoteTooLong => "Ghi chú chuyển hướng không được vượt quá 256 ký tự.",
            Self::NoteControlChars => "Ghi chú chuyển hướng không được chứa ký tự điều khiển.",
            Self::CsvEmpty => "Nội dung CSV không có dòng dữ liệu hợp lệ nào.",
            Self::CsvTooLarge => "Nội dung CSV vượt quá giới hạn tối đa 2 MiB.",
            Self::CsvTooManyRows => "Nội dung CSV vượt quá giới hạn tối đa 5.000 dòng dữ liệu.",
            Self::CsvMalformedQuote => "Nội dung CSV có dấu ngoặc kép không hợp lệ.",
            Self::CsvMissingRequiredHeaders => {
                "Tiêu đề CSV bắt buộc phải có cột source_path và target_url."
            }
            Self::CsvDuplicateMappedHeaders => {
                "Tiêu đề CSV có nhiều cột ánh xạ trùng về cùng một trường."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvRowError {
    Validation(RedirectValidationError),
    InvalidStatusValue,
    InvalidEnabledValue,
    InvalidColumnCount,
    DuplicateSourceInCsv,
    ConflictExistingSource,
    CycleOrChainExceeded,
    QuotaExceeded,
}

impl CsvRowError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Validation(v) => v.code(),
            Self::InvalidStatusValue => "redirects.csv_invalid_status_value",
            Self::InvalidEnabledValue => "redirects.csv_invalid_enabled_value",
            Self::InvalidColumnCount => "redirects.csv_invalid_column_count",
            Self::DuplicateSourceInCsv => "redirects.csv_duplicate_in_batch",
            Self::ConflictExistingSource => "redirects.conflict_existing_source",
            Self::CycleOrChainExceeded => "redirects.cycle_or_chain_exceeded",
            Self::QuotaExceeded => "redirects.quota_exceeded",
        }
    }

    pub const fn canonical_vi_message(self) -> &'static str {
        match self {
            Self::Validation(v) => v.canonical_vi_message(),
            Self::InvalidStatusValue => "Giá trị status_code trong CSV chỉ chấp nhận 301 hoặc 302.",
            Self::InvalidEnabledValue => {
                "Giá trị enabled trong CSV chỉ chấp nhận true/false, 1/0 hoặc yes/no."
            }
            Self::InvalidColumnCount => "Số lượng cột trên dòng CSV không hợp lệ.",
            Self::DuplicateSourceInCsv => "Đường dẫn nguồn bị lặp lại trong cùng tệp CSV.",
            Self::ConflictExistingSource => "Đường dẫn nguồn đã tồn tại trong bảng chuyển hướng.",
            Self::CycleOrChainExceeded => {
                "Quy tắc tạo vòng lặp chuyển hướng hoặc chuỗi dài hơn 3 bước."
            }
            Self::QuotaExceeded => "Đã đạt giới hạn tối đa 20.000 chuyển hướng thủ công.",
        }
    }
}

impl From<RedirectValidationError> for CsvRowError {
    fn from(value: RedirectValidationError) -> Self {
        Self::Validation(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum RedirectStatusCode {
    MovedPermanently = 301,
    Found = 302,
}

impl RedirectStatusCode {
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    pub const fn as_i64(self) -> i64 {
        self as i64
    }
}

impl TryFrom<u16> for RedirectStatusCode {
    type Error = RedirectValidationError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            301 => Ok(Self::MovedPermanently),
            302 => Ok(Self::Found),
            _ => Err(RedirectValidationError::InvalidStatusCode),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Skip,
    Overwrite,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CsvImportRowStatus {
    Create,
    Update,
    SkipExisting,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRedirectInput {
    pub source_path: String,
    pub target_url: String,
    pub status_code: RedirectStatusCode,
    pub enabled: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvPreviewRow {
    pub row_number: usize,
    pub source_path: String,
    pub target_url: String,
    pub status_code: Option<u16>,
    pub enabled: Option<bool>,
    pub note: String,
    pub status: CsvImportRowStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvImportRequest {
    pub csv_text: String,
    pub dry_run: bool,
    pub conflict_policy: ConflictPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CsvImportResponse {
    pub dry_run: bool,
    pub total_rows: usize,
    pub create_count: usize,
    pub update_count: usize,
    pub skip_count: usize,
    pub error_count: usize,
    pub truncated: bool,
    pub rows: Vec<CsvPreviewRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRedirectCsvRow {
    pub source_path: String,
    pub target_url: String,
    pub status_code: u16,
    pub enabled: bool,
    pub note: String,
    pub hit_count: i64,
    pub last_hit_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCsvRow {
    pub row_number: usize,
    pub input: ManualRedirectInput,
}
