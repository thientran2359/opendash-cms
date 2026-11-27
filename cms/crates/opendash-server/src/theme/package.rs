use super::*;

pub(crate) fn v3_entry_cap_and_allowlist(rel_path: &str) -> Result<usize, &'static str> {
    if rel_path == "theme.json" {
        return Ok(V2_MAX_TEXT_BYTES);
    }
    if rel_path == "functions.js" {
        return Ok(crate::plugin::package::MAX_WASM_BYTES);
    }

    let (parent, name) = match rel_path.rsplit_once('/') {
        Some((parent, name)) => (parent, name),
        None => ("", rel_path),
    };
    if let Some(rest) = rel_path.strip_prefix("blocks/") {
        if let Some((slug, leaf)) = rest.split_once('/')
            && valid_kebab_segment_with_limit(slug, 32)
            && matches!(leaf, "block.json" | "render.html")
        {
            return Ok(V2_MAX_TEXT_BYTES);
        }
        return Err("Đường dẫn blocks/<slug>/ của theme v3 không hợp lệ.");
    }
    if !parent.is_empty()
        && parent.split('/').any(|segment| {
            segment.is_empty()
                || segment.starts_with('.')
                || !segment.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_')
                })
        })
    {
        return Err("Tên thư mục theme v3 không hợp lệ.");
    }
    let is_html = name.ends_with(".html");
    let is_json = name.ends_with(".json");
    let is_l10n = name.ends_with(".pot") || name.ends_with(".po") || name.ends_with(".mo");
    let safe_file_name = name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'));
    if !safe_file_name || name.starts_with('.') || name.contains("..") {
        return Err("Tên file theme v3 không hợp lệ.");
    }
    match parent {
        "" | "template-part" | "page-layouts" if is_html => Ok(V2_MAX_TEXT_BYTES),
        "patterns" if is_json => Ok(V2_MAX_TEXT_BYTES),
        "languages" if is_l10n => Ok(1024 * 1024),
        _ if rel_path.starts_with("assets/") => {
            if !valid_asset_path(rel_path) {
                return Err("Đường dẫn assets theme v3 không đúng schema D01.");
            }
            let ext = name.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
            match ext {
                "css" | "txt" => Ok(V2_MAX_TEXT_BYTES),
                "js" | "mjs" | "svg" => Ok(V3_MAX_ACTIVE_ASSET_BYTES),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "woff2" => Ok(V2_MAX_RASTER_BYTES),
                _ => Err("Định dạng assets theme v3 không được cho phép."),
            }
        }
        _ => Err("Gói theme v3 chứa file ngoài root hoặc bốn thư mục được phép."),
    }
}

/// Decode strict, paired declarative blocks from a validated package file map.
/// This is shared by theme and plugin ingestion, and by theme activation after
/// checking the immutable revision's on-disk file digests.
pub(crate) fn collect_package_blocks(
    files: &BTreeMap<String, Vec<u8>>,
    owner_id: &str,
) -> Result<Vec<(BlockSchemaV1, String)>, &'static str> {
    let mut slugs = BTreeSet::new();
    for path in files.keys().filter(|path| path.starts_with("blocks/")) {
        let Some(rest) = path.strip_prefix("blocks/") else {
            unreachable!()
        };
        let Some((slug, leaf)) = rest.split_once('/') else {
            return Err("Đường dẫn block trong gói không hợp lệ.");
        };
        if !valid_kebab_segment_with_limit(slug, 32)
            || !matches!(leaf, "block.json" | "render.html")
        {
            return Err("Gói chứa đường dẫn block ngoài hợp đồng.");
        }
        slugs.insert(slug.to_owned());
    }
    if slugs.len() > 32 {
        return Err("Mỗi gói chỉ được khai báo tối đa 32 blocks.");
    }
    if !slugs.is_empty() && !valid_kebab_segment_with_limit(owner_id, 32) {
        return Err("Package ID khai báo block phải là kebab-case tối đa 32 ký tự.");
    }

    let mut blocks = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let schema_path = format!("blocks/{slug}/block.json");
        let template_path = format!("blocks/{slug}/render.html");
        let raw_schema =
            std::str::from_utf8(files.get(&schema_path).ok_or("Block thiếu block.json.")?)
                .map_err(|_| "block.json phải là UTF-8 hợp lệ.")?;
        let template = std::str::from_utf8(
            files
                .get(&template_path)
                .ok_or("Block thiếu render.html.")?,
        )
        .map_err(|_| "render.html phải là UTF-8 hợp lệ.")?;
        let schema: BlockSchemaV1 =
            serde_json::from_str(raw_schema).map_err(|_| "block.json không đúng schema v1.")?;
        schema.validate(owner_id)?;
        if schema.id != format!("{owner_id}/{slug}") {
            return Err("ID block không khớp thư mục blocks/<slug>.");
        }
        crate::template_sec::validate_block_template_csec(&schema.id, template, &schema)?;
        blocks.push((schema, template.to_owned()));
    }
    Ok(blocks)
}

/// Read a v3 ZIP through the shared bounded, symlink-safe package reader,
/// then validate manifest paths, content AST and C-SEC before persistence.
pub fn parse_theme_zip_v3(bytes: &[u8]) -> Result<ThemeV3Package, V2ZipError> {
    let files = read_v2_zip_entries(bytes, v3_entry_cap_and_allowlist)?;
    validate_theme_v3_files(files)
}

pub(super) fn zip_err_msg(err: V2ZipError) -> String {
    match err {
        V2ZipError::PayloadTooLarge(msg) | V2ZipError::BadRequest(msg) => msg.to_owned(),
    }
}

pub(super) fn walk_v3_dir(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    files: &mut BTreeMap<String, Vec<u8>>,
    seen_lower: &mut BTreeSet<String>,
    decoded_total: &mut usize,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Không thể đọc thư mục '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Lỗi đọc mục thư mục theme: {e}"))?;
        let file_name = entry.file_name();
        let name = file_name.to_str().ok_or_else(|| {
            "Tên file hoặc thư mục trong theme không phải UTF-8 hợp lệ.".to_owned()
        })?;
        if name.starts_with('.') {
            return Err(format!("Không cho phép entry ẩn trong theme: {name}"));
        }
        let rel_path = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let entry_path = entry.path();
        let meta = std::fs::symlink_metadata(&entry_path)
            .map_err(|e| format!("Không thể kiểm tra metadata '{rel_path}': {e}"))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(format!(
                "Không cho phép symlink bên trong theme: {rel_path}"
            ));
        }
        if ft.is_dir() {
            if depth >= MAX_THEME_DIR_DEPTH {
                return Err(format!(
                    "Thư mục theme vượt quá giới hạn độ sâu tối đa ({MAX_THEME_DIR_DEPTH}): {rel_path}"
                ));
            }
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn theme bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            walk_v3_dir(
                &entry_path,
                &rel_path,
                depth + 1,
                files,
                seen_lower,
                decoded_total,
            )?;
        } else if ft.is_file() {
            if files.len() >= V2_MAX_FILES {
                return Err("Theme v3 vượt quá giới hạn số lượng file.".to_owned());
            }
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn theme bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            if depth == 0 && name.eq_ignore_ascii_case("readme.md") {
                if meta.len() > V2_MAX_TEXT_BYTES as u64 {
                    return Err(format!(
                        "File '{rel_path}' vượt quá giới hạn kích thước ({V2_MAX_TEXT_BYTES} bytes)."
                    ));
                }
                continue;
            }
            validate_theme_relative_path(&rel_path).map_err(str::to_owned)?;
            let cap = v3_entry_cap_and_allowlist(&rel_path).map_err(str::to_owned)?;
            if meta.len() > cap as u64 {
                return Err(format!(
                    "File '{rel_path}' vượt quá giới hạn kích thước ({cap} bytes)."
                ));
            }
            let file = std::fs::File::open(&entry_path)
                .map_err(|e| format!("Không thể mở file '{rel_path}': {e}"))?;
            let mut data = Vec::with_capacity((meta.len() as usize).min(cap));
            file.take((cap + 1) as u64)
                .read_to_end(&mut data)
                .map_err(|e| format!("Không thể đọc file '{rel_path}': {e}"))?;
            if data.len() > cap {
                return Err(format!(
                    "File '{rel_path}' vượt quá giới hạn kích thước ({cap} bytes)."
                ));
            }
            *decoded_total = decoded_total.saturating_add(data.len());
            if *decoded_total > V2_MAX_DECODED_BYTES {
                return Err("Theme v3 vượt quá giới hạn tổng kích thước giải mã 32 MiB.".to_owned());
            }
            files.insert(rel_path, data);
        } else {
            return Err(format!(
                "Không cho phép entry không phải file thường trong theme: {rel_path}"
            ));
        }
    }
    Ok(())
}

pub(super) fn build_theme_check_summary(pkg: &ThemeV3Package) -> Result<ThemeCheckSummary, String> {
    let mut templates: Vec<String> = pkg
        .manifest
        .templates
        .iter()
        .map(|t| t.canonical_key().unwrap_or_else(|_| t.file.clone()))
        .collect();
    templates.sort();

    let parts: Vec<String> = pkg.manifest.parts.keys().cloned().collect();

    let mut page_layouts: Vec<String> = pkg
        .manifest
        .page_layouts
        .iter()
        .map(|l| l.key.clone())
        .collect();
    page_layouts.sort();

    let mut patterns: Vec<String> = pkg
        .manifest
        .patterns
        .iter()
        .map(|p| p.key.clone())
        .collect();
    patterns.sort();

    let mut blocks: Vec<String> = collect_package_blocks(&pkg.files, &pkg.manifest.theme_id)
        .map_err(str::to_owned)?
        .into_iter()
        .map(|(schema, _)| schema.id)
        .collect();
    blocks.sort();

    let mut assets = pkg.manifest.assets.clone();
    assets.sort();

    let options_count = crate::storage::settings::parse_theme_options_schema(&pkg.manifest_json)
        .map_err(str::to_owned)?
        .into_iter()
        .filter(|spec| !spec.is_ui_only())
        .count();

    Ok(ThemeCheckSummary {
        ok: true,
        theme_id: pkg.manifest.theme_id.clone(),
        version: pkg.manifest.version.clone(),
        templates,
        parts,
        page_layouts,
        patterns,
        blocks,
        assets,
        options_count,
        has_functions: pkg.manifest.functions.is_some(),
    })
}

pub fn check_theme_path(path: &Path) -> Result<ThemeCheckSummary, String> {
    let meta = std::fs::metadata(path).map_err(|e| {
        format!(
            "Không thể truy cập đường dẫn theme '{}': {e}",
            path.display()
        )
    })?;
    let pkg = if meta.is_file() {
        if meta.len() > V2_MAX_ZIP_BYTES as u64 {
            return Err(format!(
                "File ZIP theme vượt quá giới hạn {V2_MAX_ZIP_BYTES} bytes."
            ));
        }
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Không thể mở file ZIP '{}': {e}", path.display()))?;
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        file.take((V2_MAX_ZIP_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Không thể đọc file ZIP '{}': {e}", path.display()))?;
        if bytes.len() > V2_MAX_ZIP_BYTES {
            return Err(format!(
                "File ZIP theme vượt quá giới hạn {V2_MAX_ZIP_BYTES} bytes."
            ));
        }
        parse_theme_zip_v3(&bytes).map_err(zip_err_msg)?
    } else if meta.is_dir() {
        let files = load_v3_files_from_dir(path)?;
        validate_theme_v3_files(files).map_err(zip_err_msg)?
    } else {
        return Err(format!(
            "Đường dẫn '{}' không phải file hoặc thư mục hợp lệ.",
            path.display()
        ));
    };
    build_theme_check_summary(&pkg)
}
