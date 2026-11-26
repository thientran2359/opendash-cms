use super::*;

impl SampleGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pages => "pages",
            Self::Menus => "menus",
            Self::Media => "media",
            Self::Settings => "settings",
        }
    }
}

pub(in crate::storage) fn default_sample_key() -> String {
    "default".to_string()
}

pub(in crate::storage) fn default_page_type() -> String {
    "page".to_string()
}

pub(in crate::storage) fn valid_object_key(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 128
        && raw
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

pub(in crate::storage) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub(in crate::storage) fn infer_media_mime(
    path: &str,
    declared: Option<&str>,
) -> ApiResult<&'static str> {
    if let Some(m) = declared {
        return match m {
            "image/png" => Ok("image/png"),
            "image/jpeg" | "image/jpg" => Ok("image/jpeg"),
            "image/webp" => Ok("image/webp"),
            _ => Err(ApiError::bad_request("Định dạng media không được hỗ trợ.")),
        };
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        _ => Err(ApiError::bad_request("Định dạng media không được hỗ trợ.")),
    }
}

pub async fn execute_sample_import(
    db: &SqlitePool,
    themes_root: &std::path::Path,
    uploads_root: &std::path::Path,
    theme_id: &str,
    input: &ExecuteImportInput,
) -> ApiResult<ImportExecutionOutcome> {
    execute_sample_import_with_cutpoint(
        db,
        themes_root,
        uploads_root,
        theme_id,
        input,
        ImportCutpoint::None,
    )
    .await
}
