use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use opendash_core::l10n::{
    MAX_MO_BYTES, MAX_PO_BYTES, PotDocument, canonicalize_locale_code, compile_po_to_mo,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::{require_admin, require_admin_with_csrf};
use crate::error::{ApiError, ApiResult};
use crate::l10n::registry::{CatalogSource, L10nScope, validate_language_pack_bytes};
use crate::operations::AdmittedMutation;
use crate::paths::SitePaths;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

const MAX_UPLOAD_BODY_BYTES: usize = 3 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct PackSummary {
    locale: String,
    source: &'static str,
    entries: usize,
    coverage_pct: u8,
    has_override: bool,
    etag: String,
}

#[derive(Debug, Serialize)]
struct DomainPacksSummary {
    scope: &'static str,
    domain: String,
    name: String,
    reference_count: Option<usize>,
    packs: Vec<PackSummary>,
}

#[derive(Debug, Deserialize)]
pub struct UploadLanguagePackRequest {
    pub scope: String,
    pub domain: String,
    pub locale: String,
    pub filename: String,
    pub content_base64: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/l10n/packs", get(l10n_packs_list))
        .route(
            "/api/admin/l10n/packs/upload",
            post(l10n_pack_upload).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route(
            "/api/admin/l10n/packs/{scope}/{domain}/{locale}",
            delete(l10n_pack_delete),
        )
}

fn parse_scope(raw: &str) -> ApiResult<L10nScope> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "core" => Ok(L10nScope::Core),
        "theme" => Ok(L10nScope::Theme),
        "plugin" => Ok(L10nScope::Plugin),
        _ => Err(ApiError::bad_request(
            "Phạm vi language pack phải là core, theme hoặc plugin.",
        )),
    }
}

fn scope_label(scope: L10nScope) -> &'static str {
    match scope {
        L10nScope::Core => "core",
        L10nScope::Theme => "theme",
        L10nScope::Plugin => "plugin",
    }
}

fn source_label(source: CatalogSource) -> &'static str {
    match source {
        CatalogSource::SiteOverride => "site_override",
        CatalogSource::DevOverlay => "dev_overlay",
        CatalogSource::PackageBundled => "package_bundled",
        CatalogSource::EmbeddedCore => "embedded_core",
    }
}

fn validate_domain(scope: L10nScope, raw: &str) -> ApiResult<String> {
    let trimmed = raw.trim();
    if scope == L10nScope::Core {
        if trimmed != "opendash" {
            return Err(ApiError::bad_request(
                "Domain của core language pack phải là 'opendash'.",
            ));
        }
        return Ok("opendash".to_owned());
    }
    if !opendash_core::id::valid_package_id(trimmed) {
        return Err(ApiError::bad_request("Text domain không hợp lệ."));
    }
    Ok(trimmed.to_owned())
}

fn scope_dir(paths: &SitePaths, scope: L10nScope) -> PathBuf {
    match scope {
        L10nScope::Core => paths.core_languages_dir(),
        L10nScope::Theme => paths.theme_languages_dir(),
        L10nScope::Plugin => paths.plugin_languages_dir(),
    }
}

fn pack_stem(_scope: L10nScope, domain: &str, locale: &str) -> String {
    format!("{domain}-{locale}")
}

fn has_site_override(paths: &SitePaths, scope: L10nScope, domain: &str, locale: &str) -> bool {
    let dir = scope_dir(paths, scope);
    let stem = pack_stem(scope, domain, locale);
    dir.join(format!("{stem}.mo")).is_file() || dir.join(format!("{stem}.po")).is_file()
}

fn write_file_atomic(target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Không thể tạo thư mục: {e}"))?;
    }
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Tên file không hợp lệ.".to_owned())?;
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = target.with_file_name(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));
    {
        let mut f = File::create(&tmp_path).map_err(|e| format!("Không thể tạo file tạm: {e}"))?;
        f.write_all(bytes)
            .map_err(|e| format!("Không thể ghi file tạm: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("Không thể fsync file tạm: {e}"))?;
    }
    if let Err(err) = fs::rename(&tmp_path, target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Không thể thay thế file language pack: {err}"));
    }
    Ok(())
}

fn decode_base64(input: &str) -> ApiResult<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.len() % 4 != 0 {
        return Err(ApiError::bad_request("Dữ liệu base64 không hợp lệ."));
    }
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for chunk in clean.chunks_exact(4) {
        let a = val(chunk[0]).ok_or_else(|| ApiError::bad_request("Base64 không hợp lệ."))?;
        let b = val(chunk[1]).ok_or_else(|| ApiError::bad_request("Base64 không hợp lệ."))?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2]).ok_or_else(|| ApiError::bad_request("Base64 không hợp lệ."))?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d =
                    val(chunk[3]).ok_or_else(|| ApiError::bad_request("Base64 không hợp lệ."))?;
                out.push((c << 6) | d);
            }
        } else if chunk[3] != b'=' {
            return Err(ApiError::bad_request("Padding base64 không hợp lệ."));
        }
    }
    Ok(out)
}

fn reload_catalog_registry(state: &AppState) -> ApiResult<()> {
    let mut embedded = BTreeMap::new();
    embedded.insert("vi".to_owned(), crate::l10n::EMBEDDED_VI_MO.to_vec());
    state
        .catalog_registry
        .reload_from_site_paths(
            state.coordinator.paths(),
            embedded,
            BTreeMap::new(),
            BTreeMap::new(),
        )
        .map_err(|e| ApiError::internal(format!("Không thể nạp lại catalog l10n: {e}")))
}

pub async fn l10n_packs_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    require_admin(&headers, &state.db).await?;
    let core_ref_count = PotDocument::parse(include_str!("../../../../l10n/opendash.pot"))
        .map(|doc| doc.entries.len())
        .unwrap_or(0);

    let mut domains: BTreeMap<(L10nScope, String), DomainPacksSummary> = BTreeMap::new();
    domains.insert(
        (L10nScope::Core, "opendash".to_owned()),
        DomainPacksSummary {
            scope: "core",
            domain: "opendash".to_owned(),
            name: "OpenDash Core".to_owned(),
            reference_count: Some(core_ref_count),
            packs: Vec::new(),
        },
    );

    if let Ok(sel) = crate::storage::theme::get_active_selection(&state.db).await
        && let Some(theme_id) = sel.theme_id
    {
        domains.insert(
            (L10nScope::Theme, theme_id.clone()),
            DomainPacksSummary {
                scope: "theme",
                domain: theme_id.clone(),
                name: theme_id,
                reference_count: None,
                packs: Vec::new(),
            },
        );
    }

    if let Ok(plugins) = crate::storage::plugin::list_plugins(&state.db).await {
        for p in plugins {
            domains.insert(
                (L10nScope::Plugin, p.id.clone()),
                DomainPacksSummary {
                    scope: "plugin",
                    domain: p.id,
                    name: p.name,
                    reference_count: None,
                    packs: Vec::new(),
                },
            );
        }
    }

    let snap = state.catalog_registry.snapshot();
    let paths = state.coordinator.paths();
    for (key, loaded) in &snap.catalogs {
        let entry = domains
            .entry((key.scope, key.domain.clone()))
            .or_insert_with(|| DomainPacksSummary {
                scope: scope_label(key.scope),
                domain: key.domain.clone(),
                name: key.domain.clone(),
                reference_count: (key.scope == L10nScope::Core && key.domain == "opendash")
                    .then_some(core_ref_count),
                packs: Vec::new(),
            });
        let entries_count = loaded.catalog.entries().len();
        let coverage_pct = match entry.reference_count {
            Some(total) if total > 0 => ((entries_count.min(total) * 100) / total) as u8,
            _ => 100,
        };
        entry.packs.push(PackSummary {
            locale: key.locale.clone(),
            source: source_label(loaded.source),
            entries: entries_count,
            coverage_pct,
            has_override: has_site_override(paths, key.scope, &key.domain, &key.locale),
            etag: loaded.sha256_etag.clone(),
        });
    }

    for dom in domains.values_mut() {
        dom.packs.sort_by(|a, b| a.locale.cmp(&b.locale));
    }

    let items: Vec<DomainPacksSummary> = domains.into_values().collect();
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "domains": items })),
    )
        .into_response())
}

pub async fn l10n_pack_upload(
    State(state): State<AppState>,
    crate::operations::AdmittedMutation(_lease): AdmittedMutation,
    headers: HeaderMap,
    Json(req): Json<UploadLanguagePackRequest>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &state.db).await?;
    let scope = parse_scope(&req.scope)?;
    let domain = validate_domain(scope, &req.domain)?;
    let locale = canonicalize_locale_code(&req.locale)
        .map_err(|_| ApiError::bad_request("Mã locale BCP47 không hợp lệ."))?;

    let filename = req.filename.trim();
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(ApiError::bad_request("Tên file không hợp lệ."));
    }
    let lower_name = filename.to_ascii_lowercase();
    let is_po = if lower_name.ends_with(".po") {
        true
    } else if lower_name.ends_with(".mo") {
        false
    } else {
        return Err(ApiError::bad_request(
            "Chỉ hỗ trợ tải lên file .po hoặc .mo.",
        ));
    };

    let bytes = decode_base64(&req.content_base64)?;
    let max_bytes = if is_po { MAX_PO_BYTES } else { MAX_MO_BYTES };
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(ApiError::bad_request(
            "Kích thước file language pack vượt quá giới hạn 2 MiB hoặc rỗng.",
        ));
    }
    validate_language_pack_bytes(is_po, &bytes)
        .map_err(|e| ApiError::bad_request(format!("File language pack không hợp lệ: {e}")))?;

    let dir = scope_dir(state.coordinator.paths(), scope);
    let stem = pack_stem(scope, &domain, &locale);
    let mo_path = dir.join(format!("{stem}.mo"));
    let po_path = dir.join(format!("{stem}.po"));

    if is_po {
        let po_text = std::str::from_utf8(&bytes)
            .map_err(|_| ApiError::bad_request("File .po phải là UTF-8 hợp lệ."))?;
        let mo_bytes = compile_po_to_mo(po_text)
            .map_err(|e| ApiError::bad_request(format!("Không thể biên dịch file .po: {e}")))?;
        write_file_atomic(&po_path, &bytes).map_err(ApiError::internal)?;
        write_file_atomic(&mo_path, &mo_bytes).map_err(ApiError::internal)?;
    } else {
        write_file_atomic(&mo_path, &bytes).map_err(ApiError::internal)?;
        if po_path.exists() {
            let _ = fs::remove_file(&po_path);
        }
    }

    reload_catalog_registry(&state)?;
    let loaded = state
        .catalog_registry
        .resolve(scope, &domain, &locale)
        .map(|c| c.catalog.entries().len())
        .unwrap_or(0);

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "scope": scope_label(scope),
            "domain": domain,
            "locale": locale,
            "entries": loaded,
        })),
    )
        .into_response())
}

pub async fn l10n_pack_delete(
    State(state): State<AppState>,
    crate::operations::AdmittedMutation(_lease): AdmittedMutation,
    headers: HeaderMap,
    AxumPath((raw_scope, raw_domain, raw_locale)): AxumPath<(String, String, String)>,
) -> ApiResult<Response> {
    require_admin_with_csrf(&headers, &state.db).await?;
    let scope = parse_scope(&raw_scope)?;
    let domain = validate_domain(scope, &raw_domain)?;
    let locale = canonicalize_locale_code(&raw_locale)
        .map_err(|_| ApiError::bad_request("Mã locale BCP47 không hợp lệ."))?;

    let dir = scope_dir(state.coordinator.paths(), scope);
    let stem = pack_stem(scope, &domain, &locale);
    let po_path = dir.join(format!("{stem}.po"));
    let mo_path = dir.join(format!("{stem}.mo"));
    let mut removed = false;
    for path in [&po_path, &mo_path] {
        if path.exists() {
            fs::remove_file(path)
                .map_err(|e| ApiError::internal(format!("Không thể xóa language pack: {e}")))?;
            removed = true;
        }
    }
    if !removed {
        return Err(ApiError::not_found(
            "Không tìm thấy language pack tùy chỉnh để xóa.",
        ));
    }

    reload_catalog_registry(&state)?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "scope": scope_label(scope),
            "domain": domain,
            "locale": locale,
        })),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct AdminCatalogQuery {
    pub locale: Option<String>,
}

pub async fn admin_i18n_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminCatalogQuery>,
) -> ApiResult<Response> {
    let raw_locale = query.locale.as_deref().unwrap_or("en");
    let Some(opt) = opendash_core::i18n::find_cms_locale(raw_locale) else {
        return Err(ApiError::bad_request("Mã ngôn ngữ Admin không hợp lệ."));
    };
    let snap = state.catalog_registry.snapshot();
    let (etag, body_bytes): (String, Vec<u8>) =
        if let Some(cached) = snap.admin_json_cache.get(opt.code) {
            (cached.etag.clone(), (*cached.json_bytes).clone())
        } else if opt.code == "en" {
            let etag = "\"en-builtin\"".to_owned();
            let body = serde_json::to_vec(&serde_json::json!({
                "locale": "en",
                "etag": etag,
                "nplurals": 2,
                "plural_forms": "nplurals=2; plural=(n != 1);",
                "entries": {},
                "context_entries": {},
                "plural_entries": {},
                "context_plural_entries": {}
            }))
            .unwrap_or_else(|_| b"{}".to_vec());
            (etag, body)
        } else {
            return Err(ApiError::not_found("Gói ngôn ngữ chưa được cài đặt."));
        };

    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|inm| inm.trim() == etag)
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (
                    header::CACHE_CONTROL,
                    "public, max-age=0, must-revalidate".to_owned(),
                ),
            ],
        )
            .into_response());
    }

    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/json; charset=utf-8".to_owned(),
            ),
            (header::ETAG, etag),
            (
                header::CACHE_CONTROL,
                "public, max-age=0, must-revalidate".to_owned(),
            ),
        ],
        body_bytes,
    )
        .into_response())
}
