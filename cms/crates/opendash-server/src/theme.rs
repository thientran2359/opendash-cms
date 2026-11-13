use std::collections::BTreeMap;

use minijinja::{AutoEscape, Environment, UndefinedBehavior};
pub use opendash_core::theme::{Block, BlockDescriptor, Fields};
use serde::{Deserialize, Serialize};

use crate::{
    package::{
        V2_MAX_RASTER_BYTES, V2_MAX_TEXT_BYTES, V2_MAX_ZIP_BYTES, V2ZipError,
        ensure_safe_flat_entry, open_bounded_zip, read_bounded_entry, read_v2_zip_entries,
        validate_v2_relative_path,
    },
    storage::theme::valid_v2_theme_id,
};

pub const MAX_ZIP_BYTES: usize = 64 * 1024;

pub struct Theme {
    pub descriptor: BlockDescriptor,
    pub template: String,
    pub content_list_template: Option<String>,
    pub content_single_template: Option<String>,
    /// Explicitly supported block from another installed theme; never rewrites home_page.
    pub compatible_block_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HomepageCompat {
    version: u8,
    accepts: String,
}

impl Theme {
    pub fn from_zip(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut archive = open_bounded_zip(
            bytes,
            MAX_ZIP_BYTES,
            "ZIP vượt quá giới hạn kích thước.",
            "ZIP không hợp lệ.",
        )?;
        if !(2..=5).contains(&archive.len()) {
            return Err(
                "ZIP cần block.json, block.html và chỉ thêm content-list.html/content-single.html/homepage-compat.json.",
            );
        }

        let mut descriptor = None;
        let mut template = None;
        let mut content_list_template = None;
        let mut content_single_template = None;
        let mut homepage_compat = None;
        let mut remaining = MAX_ZIP_BYTES;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|_| "Không đọc được ZIP.")?;
            let name = ensure_safe_flat_entry(
                &entry,
                "Đường dẫn ZIP không an toàn.",
                "ZIP không được chứa symlink.",
                "ZIP không được chứa thư mục con.",
            )?;
            let slot = match name.as_str() {
                "block.json" if descriptor.is_none() => &mut descriptor,
                "block.html" if template.is_none() => &mut template,
                "content-list.html" if content_list_template.is_none() => {
                    &mut content_list_template
                }
                "content-single.html" if content_single_template.is_none() => {
                    &mut content_single_template
                }
                "homepage-compat.json" if homepage_compat.is_none() => &mut homepage_compat,
                _ => return Err("ZIP chứa đường dẫn lạ hoặc mục bị trùng."),
            };
            if entry.size() > remaining as u64 {
                return Err("Nội dung ZIP vượt quá giới hạn kích thước.");
            }
            let raw_bytes = read_bounded_entry(
                &mut entry,
                remaining,
                "Không đọc được nội dung ZIP dạng UTF-8.",
            )?;
            let content = String::from_utf8(raw_bytes)
                .map_err(|_| "Không đọc được nội dung ZIP dạng UTF-8.")?;
            remaining = remaining
                .checked_sub(content.len())
                .ok_or("Nội dung ZIP vượt quá giới hạn kích thước.")?;
            *slot = Some(content);
        }

        let descriptor: BlockDescriptor =
            serde_json::from_str(&descriptor.ok_or("Thiếu block.json.")?)
                .map_err(|_| "block.json không hợp lệ.")?;
        descriptor.validate()?;
        let compatible_block_id = if let Some(raw) = homepage_compat {
            let compat: HomepageCompat =
                serde_json::from_str(&raw).map_err(|_| "homepage-compat.json không hợp lệ.")?;
            if compat.version != 1
                || !opendash_core::id::valid_namespaced_block_id(&compat.accepts)
                || compat.accepts == descriptor.id
            {
                return Err("Alias homepage phải là một block khác có ID hợp lệ (version 1).");
            }
            Some(compat.accepts)
        } else {
            None
        };

        let template = template.ok_or("Thiếu block.html.")?;
        if template.trim().is_empty() || environment().template_from_str(&template).is_err() {
            return Err("Template block không hợp lệ.");
        }
        for optional in [&content_list_template, &content_single_template] {
            if let Some(template) = optional {
                if template.trim().is_empty() || environment().template_from_str(template).is_err()
                {
                    return Err("Template CPT không hợp lệ.");
                }
            }
        }
        Ok(Self {
            descriptor,
            template,
            content_list_template,
            content_single_template,
            compatible_block_id,
        })
    }
}

pub fn render(template: &str, fields: &Fields, block: &Block) -> Result<String, &'static str> {
    block.validate_against_fields(fields)?;
    let env = environment();
    env.template_from_str(template)
        .and_then(|template| template.render(minijinja::context!(values => &block.values)))
        .map_err(|_| "Không render được template block.")
}

/// Shared MiniJinja HTML auto-escaping and fuel budget for theme CPT templates
/// and the built-in fallback, matching the Builder's rendering rules.
pub fn render_content(template: &str, data: &serde_json::Value) -> Result<String, &'static str> {
    let env = environment();
    env.template_from_str(template)
        .and_then(|t| t.render(json_to_template_value(data)))
        .map_err(|_| "Không thể render template nội dung.")
}

/// Convert only ordinary JSON scalars/arrays/objects into the restricted
/// MiniJinja Value context, never into a marked-safe HTML string.
fn json_to_template_value(value: &serde_json::Value) -> minijinja::Value {
    use minijinja::Value;
    match value {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(v) => Value::from(*v),
        serde_json::Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Value::from(i)
            } else if let Some(u) = v.as_u64() {
                Value::from(u)
            } else {
                Value::from(v.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(v) => Value::from(v.clone()),
        serde_json::Value::Array(v) => {
            Value::from(v.iter().map(json_to_template_value).collect::<Vec<_>>())
        }
        serde_json::Value::Object(v) => Value::from_pairs(
            v.iter()
                .map(|(key, value)| (key.clone(), json_to_template_value(value))),
        ),
    }
}

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(10_000));
    env
}

const ALLOWED_V2_ROLES: &[&str] = &[
    "home",
    "page",
    "header",
    "footer",
    "archive",
    "single",
    "taxonomy_archive",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeV2Manifest {
    pub manifest_version: u8,
    pub theme_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub templates: BTreeMap<String, String>,
    #[serde(default)]
    pub options: Option<serde_json::Value>,
    #[serde(default)]
    pub menus: Option<serde_json::Value>,
    #[serde(default)]
    pub sample: Option<serde_json::Value>,
    #[serde(default)]
    pub declarations: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ThemeV2Package {
    pub manifest: ThemeV2Manifest,
    pub manifest_json: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Returns `true` if the ZIP contains `theme.json` (v2), `false` if v1 legacy (`block.json`).
/// Rejects ambiguous packages containing both manifests.
pub fn is_v2_theme_zip(bytes: &[u8]) -> Result<bool, V2ZipError> {
    let mut archive = open_bounded_zip(
        bytes,
        V2_MAX_ZIP_BYTES,
        "Gói theme ZIP vượt quá giới hạn kích thước.",
        "ZIP không hợp lệ.",
    )
    .map_err(|msg| {
        if bytes.len() > V2_MAX_ZIP_BYTES {
            V2ZipError::PayloadTooLarge(msg)
        } else {
            V2ZipError::BadRequest(msg)
        }
    })?;

    let mut has_v2 = false;
    let mut has_v1 = false;
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|_| V2ZipError::BadRequest("Không đọc được mục trong ZIP."))?;
        if let Ok(name) = entry.name() {
            if name == "theme.json" {
                has_v2 = true;
            } else if name == "block.json" {
                has_v1 = true;
            }
        }
    }
    if has_v2 && has_v1 {
        return Err(V2ZipError::BadRequest(
            "Gói theme không được chứa đồng thời theme.json (v2) và block.json (v1).",
        ));
    }
    Ok(has_v2)
}

fn v2_entry_cap_and_allowlist(rel_path: &str) -> Result<usize, &'static str> {
    let ext = rel_path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" | "html" | "htm" | "jinja" | "jinja2" | "css" | "txt" | "md" => {
            Ok(V2_MAX_TEXT_BYTES)
        }
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2" => Ok(V2_MAX_RASTER_BYTES),
        _ => Err("Loại file trong gói theme v2 không nằm trong danh sách cho phép."),
    }
}

pub fn parse_theme_zip_v2(bytes: &[u8]) -> Result<ThemeV2Package, V2ZipError> {
    let files = read_v2_zip_entries(bytes, v2_entry_cap_and_allowlist)?;
    let raw_manifest = files
        .get("theme.json")
        .ok_or(V2ZipError::BadRequest("Thiếu theme.json trong gói theme v2."))?;
    let manifest_json = std::str::from_utf8(raw_manifest)
        .map_err(|_| V2ZipError::BadRequest("theme.json phải là UTF-8 hợp lệ."))?
        .to_string();
    let manifest: ThemeV2Manifest = serde_json::from_str(&manifest_json)
        .map_err(|_| V2ZipError::BadRequest("theme.json không đúng cấu trúc v2."))?;

    if manifest.manifest_version != 2 {
        return Err(V2ZipError::BadRequest("manifest_version phải bằng 2."));
    }
    if !valid_v2_theme_id(&manifest.theme_id) {
        return Err(V2ZipError::BadRequest(
            "theme_id v2 phải là lowercase kebab-case (1–64 ký tự) và không dùng tiền tố legacy.",
        ));
    }
    if manifest.name.trim().is_empty() || manifest.name.len() > 128 {
        return Err(V2ZipError::BadRequest("Tên theme v2 không hợp lệ."));
    }
    if manifest.version.trim().is_empty()
        || manifest.version.len() > 64
        || !manifest.version.is_ascii()
    {
        return Err(V2ZipError::BadRequest("Phiên bản theme v2 không hợp lệ."));
    }

    let env = environment();
    for (role, tpl_path) in &manifest.templates {
        if !ALLOWED_V2_ROLES.contains(&role.as_str()) {
            return Err(V2ZipError::BadRequest(
                "Vai trò template trong theme.json không hợp lệ.",
            ));
        }
        let safe_tpl = validate_v2_relative_path(tpl_path).map_err(V2ZipError::BadRequest)?;
        let tpl_bytes = files.get(&safe_tpl).ok_or(V2ZipError::BadRequest(
            "File template khai báo trong theme.json không tồn tại trong ZIP.",
        ))?;
        let tpl_src = std::str::from_utf8(tpl_bytes)
            .map_err(|_| V2ZipError::BadRequest("Template phải là UTF-8 hợp lệ."))?;
        if tpl_src.trim().is_empty() || env.template_from_str(tpl_src).is_err() {
            return Err(V2ZipError::BadRequest(
                "Cú pháp template MiniJinja không hợp lệ.",
            ));
        }
    }

    Ok(ThemeV2Package {
        manifest,
        manifest_json,
        files,
    })
}

pub fn manifest_has_declarations(manifest_json: &str) -> bool {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(manifest_json) else {
        return false;
    };
    let Some(decl) = val.get("declarations") else {
        return false;
    };
    !decl.is_null()
        && !matches!(decl, serde_json::Value::Object(m) if m.is_empty())
        && !matches!(decl, serde_json::Value::Array(a) if a.is_empty())
}

pub fn theme_asset_mime(rel_path: &str) -> Option<&'static str> {
    let ext = rel_path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "css" => Some("text/css; charset=utf-8"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

pub fn theme_asset_url(revision_digest: &str, rel_path: &str) -> String {
    format!("/theme/{revision_digest}/{rel_path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn build_zip(entries: &[(&str, &[u8], CompressionMethod)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            for (name, data, method) in entries {
                let opts = SimpleFileOptions::default().compression_method(*method);
                zw.start_file(*name, opts).unwrap();
                zw.write_all(data).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parses_valid_v2_theme_and_detects_declarations() {
        let manifest = br#"{
            "manifest_version": 2,
            "theme_id": "editorial-clean",
            "name": "Editorial Clean",
            "version": "1.0.0",
            "templates": { "single": "templates/single.html" },
            "declarations": { "content_types": [{"key": "portfolio"}] }
        }"#;
        let zip = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Deflated),
            (
                "templates/single.html",
                b"<article>{{ entry.title }}</article>",
                CompressionMethod::Deflated,
            ),
            (
                "assets/style.css",
                b"body { color: #111; }",
                CompressionMethod::Deflated,
            ),
        ]);
        assert!(is_v2_theme_zip(&zip).unwrap());
        let pkg = parse_theme_zip_v2(&zip).unwrap();
        assert_eq!(pkg.manifest.theme_id, "editorial-clean");
        assert!(manifest_has_declarations(&pkg.manifest_json));
        assert_eq!(
            theme_asset_mime("assets/style.css"),
            Some("text/css; charset=utf-8")
        );
        assert_eq!(theme_asset_mime("theme.json"), None);
    }

    #[test]
    fn rejects_malicious_v2_zip_corpus() {
        let manifest =
            br#"{"manifest_version":2,"theme_id":"bad-theme","name":"Bad","version":"1.0.0"}"#;

        let traversal = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("../escape.css", b"body{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&traversal),
            Err(V2ZipError::BadRequest(_))
        ));

        let backslash = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets\\style.css", b"body{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&backslash),
            Err(V2ZipError::BadRequest(_))
        ));

        let collision = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/app.css", b"a{}", CompressionMethod::Stored),
            ("Assets/App.css", b"b{}", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&collision),
            Err(V2ZipError::BadRequest(_))
        ));

        let js = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/evil.js", b"alert(1)", CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&js),
            Err(V2ZipError::BadRequest(_))
        ));

        let big_css = vec![b'a'; V2_MAX_TEXT_BYTES + 1];
        let oversized = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Stored),
            ("assets/big.css", &big_css, CompressionMethod::Stored),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&oversized),
            Err(V2ZipError::PayloadTooLarge(_))
        ));

        let bomb_payload = vec![b'0'; 200 * 1024];
        let bomb = build_zip(&[
            ("theme.json", manifest, CompressionMethod::Deflated),
            ("assets/bomb.css", &bomb_payload, CompressionMethod::Deflated),
        ]);
        assert!(matches!(
            parse_theme_zip_v2(&bomb),
            Err(V2ZipError::BadRequest(_))
        ));
    }
}
