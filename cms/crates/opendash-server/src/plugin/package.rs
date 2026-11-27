use opendash_core::block_schema::BlockSchemaV1;

use std::{
    collections::{BTreeMap, HashSet},
    io::Read as _,
    path::Path,
};

use wasmi::{Engine, ExternType, Module, ValType};

use crate::package::{
    V2_MAX_EXPANSION_RATIO, open_bounded_zip, read_bounded_entry, validate_theme_relative_path,
};

use super::manifest::PluginManifest;

pub const MAX_PLUGIN_ZIP_BYTES: usize = 5 * 1024 * 1024;

pub const MAX_UNCOMPRESSED_TOTAL_BYTES: usize = 10 * 1024 * 1024;

pub const MAX_WASM_BYTES: usize = 2 * 1024 * 1024;

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024; // 64 KB

pub const MAX_PLUGIN_FILES: usize = 104;

pub const MAX_PLUGIN_DIR_DEPTH: usize = 4;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct PluginCheckSummary {
    pub ok: bool,
    pub plugin_id: String,
    pub version: String,
    pub manifest_schema: u32,
    pub hooks: Vec<String>,
    pub filters: Vec<String>,
    pub actions: Vec<String>,
    pub blocks: Vec<String>,
    pub settings_count: usize,
}

pub(crate) fn plugin_entry_max_bytes(name: &str) -> Option<usize> {
    if name == "manifest.json" {
        Some(MAX_MANIFEST_BYTES)
    } else if name == "plugin.wasm" || name == "plugin.js" {
        Some(MAX_WASM_BYTES)
    } else if name == "README.md" {
        Some(MAX_UNCOMPRESSED_TOTAL_BYTES)
    } else if name.starts_with("blocks/") {
        Some(crate::package::V2_MAX_TEXT_BYTES)
    } else if name.starts_with("languages/")
        && (name.ends_with(".pot") || name.ends_with(".po") || name.ends_with(".mo"))
    {
        Some(256 * 1024)
    } else {
        None
    }
}

pub fn validate_wasm_module_bytes(engine: &Engine, wasm_bytes: &[u8]) -> Result<(), &'static str> {
    let module = Module::new(engine, wasm_bytes)
        .map_err(|_| "Module WebAssembly không hợp lệ hoặc cấu trúc bị lỗi.")?;

    let run_export = module.exports().find(|e| e.name() == "opendash_run");
    let Some(run_export) = run_export else {
        return Err("Module WebAssembly phải export 'opendash_run'.");
    };
    match run_export.ty() {
        ExternType::Func(func_ty) => {
            if !func_ty.params().is_empty() || func_ty.results() != [ValType::I32] {
                return Err("Hàm export 'opendash_run' phải có signature () -> i32.");
            }
        }
        _ => return Err("Export 'opendash_run' phải là function."),
    }

    let memory_export = module.exports().find(|e| e.name() == "memory");
    let Some(memory_export) = memory_export else {
        return Err("Module WebAssembly phải export 'memory'.");
    };
    if !matches!(memory_export.ty(), ExternType::Memory(_)) {
        return Err("Export 'memory' phải là linear memory.");
    }

    for import in module.imports() {
        let (expected_params, expected_results, non_func_err, bad_sig_err): (
            &[ValType],
            &[ValType],
            &'static str,
            &'static str,
        ) = match (import.module(), import.name()) {
            ("opendash_v1", "host_get_input_len") => (
                &[],
                &[ValType::I32],
                "Import 'opendash_v1:host_get_input_len' phải là hàm.",
                "Import 'opendash_v1:host_get_input_len' sai chữ ký hàm.",
            ),
            ("opendash_v1", "host_read_input") => (
                &[ValType::I32, ValType::I32],
                &[ValType::I32],
                "Import 'opendash_v1:host_read_input' phải là hàm.",
                "Import 'opendash_v1:host_read_input' sai chữ ký hàm.",
            ),
            ("opendash_v1", "host_write_output") => (
                &[ValType::I32, ValType::I32],
                &[ValType::I32],
                "Import 'opendash_v1:host_write_output' phải là hàm.",
                "Import 'opendash_v1:host_write_output' sai chữ ký hàm.",
            ),
            ("opendash_v1", "host_call") => (
                &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
                &[ValType::I32],
                "Import 'opendash_v1:host_call' phải là hàm.",
                "Import 'opendash_v1:host_call' sai chữ ký hàm.",
            ),
            ("opendash_v1", "host_last_resp_len") => (
                &[],
                &[ValType::I32],
                "Import 'opendash_v1:host_last_resp_len' phải là hàm.",
                "Import 'opendash_v1:host_last_resp_len' sai chữ ký hàm.",
            ),
            ("opendash_v1", "host_read_last_resp") => (
                &[ValType::I32, ValType::I32],
                &[ValType::I32],
                "Import 'opendash_v1:host_read_last_resp' phải là hàm.",
                "Import 'opendash_v1:host_read_last_resp' sai chữ ký hàm.",
            ),
            ("env", "abort") => (
                &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
                &[],
                "Import 'env:abort' phải là hàm.",
                "Import 'env:abort' sai chữ ký hàm.",
            ),
            _ => {
                return Err(
                    "Module WebAssembly chứa import không được phép (từ chối WASI/unknown env).",
                );
            }
        };

        let ExternType::Func(func_ty) = import.ty() else {
            return Err(non_func_err);
        };
        if func_ty.params() != expected_params || func_ty.results() != expected_results {
            return Err(bad_sig_err);
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct PluginPackage {
    pub manifest: PluginManifest,
    pub admin_extensions: Option<opendash_core::admin_extensions::AdminExtensions>,
    pub wasm_bytes: Vec<u8>,
    pub blocks: Vec<(BlockSchemaV1, String)>,
}

impl PluginPackage {
    pub fn from_zip(bytes: &[u8], engine: &Engine) -> Result<Self, &'static str> {
        let mut archive = open_bounded_zip(
            bytes,
            MAX_PLUGIN_ZIP_BYTES,
            "Kích thước file ZIP vượt quá giới hạn 5 MB.",
            "ZIP không hợp lệ.",
        )?;
        if archive.len() > 256 {
            return Err("ZIP chứa quá nhiều entry (tối đa 256).");
        }

        let mut manifest_content: Option<String> = None;
        let mut wasm_content: Option<Vec<u8>> = None;
        let mut js_content: Option<Vec<u8>> = None;
        let mut total_actual_uncompressed: usize = 0;
        let mut seen_lower = HashSet::new();
        let mut block_files = BTreeMap::new();
        let mut regular_files = 0usize;

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|_| "Không thể đọc mục trong ZIP.")?;
            if let Some(mode) = file.unix_mode()
                && (mode & 0o170000) == 0o120000
            {
                return Err("ZIP chứa symlink bị cấm.");
            }
            let name = file
                .name()
                .map_err(|_| "ZIP chứa đường dẫn UTF-8 không hợp lệ.")?
                .to_string();
            if file.is_dir() {
                let trimmed = name.trim_end_matches('/');
                if !trimmed.is_empty() {
                    validate_theme_relative_path(trimmed)?;
                }
                if file.size() != 0 {
                    return Err("Thư mục trong ZIP phải rỗng.");
                }
                continue;
            }
            let name = validate_theme_relative_path(&name)?;
            regular_files += 1;
            if regular_files > MAX_PLUGIN_FILES {
                return Err("ZIP chứa quá nhiều file (tối đa 72 file).");
            }
            if !seen_lower.insert(name.to_ascii_lowercase()) {
                return Err("ZIP chứa mục trùng lặp hoặc xung đột chữ hoa/thường.");
            }

            // Only contract-supported files allowed
            let max_file_limit = plugin_entry_max_bytes(&name)
                .ok_or("ZIP chứa file không được hỗ trợ trong hợp đồng ABI v1.")?;
            let is_block_file = name.starts_with("blocks/");

            let remaining_total =
                MAX_UNCOMPRESSED_TOTAL_BYTES.saturating_sub(total_actual_uncompressed);
            let read_cap = std::cmp::min(max_file_limit, remaining_total);
            if file.size() > read_cap as u64 {
                return Err("File plugin vượt quá giới hạn giải nén.");
            }
            let compressed_size = file.compressed_size();

            let buffer =
                read_bounded_entry(&mut file, read_cap, "Lỗi khi đọc nội dung file trong ZIP.")?;
            if (compressed_size == 0 && !buffer.is_empty())
                || (compressed_size > 0
                    && (buffer.len() as u64)
                        > compressed_size.saturating_mul(V2_MAX_EXPANSION_RATIO))
            {
                return Err("ZIP chứa tỷ lệ giải nén bất thường.");
            }

            if buffer.len() > max_file_limit {
                return match name.as_str() {
                    "manifest.json" => Err("manifest.json vượt quá giới hạn kích thước 64 KB."),
                    "plugin.wasm" => Err("File plugin.wasm vượt quá giới hạn kích thước 2 MB."),
                    "plugin.js" => Err("File plugin.js vượt quá giới hạn kích thước 2 MB."),
                    _ => Err("File trong ZIP vượt quá giới hạn kích thước."),
                };
            }

            total_actual_uncompressed = total_actual_uncompressed
                .checked_add(buffer.len())
                .ok_or("Dung lượng giải nén vượt quá giới hạn.")?;

            if total_actual_uncompressed > MAX_UNCOMPRESSED_TOTAL_BYTES {
                return Err("Tổng dung lượng giải nén thực tế vượt quá giới hạn 10 MB.");
            }

            match name.as_str() {
                "manifest.json" => {
                    let text = String::from_utf8(buffer)
                        .map_err(|_| "manifest.json không phải UTF-8 hợp lệ.")?;
                    manifest_content = Some(text);
                }
                "plugin.wasm" => {
                    wasm_content = Some(buffer);
                }
                "plugin.js" => {
                    js_content = Some(buffer);
                }
                _ if is_block_file => {
                    block_files.insert(name, buffer);
                }
                _ => {
                    // Allowed optional files like README.md
                }
            }
        }

        let manifest_text = manifest_content.ok_or("ZIP thiếu file manifest.json.")?;
        Self::finish_from_raw_parts(manifest_text, wasm_content, js_content, block_files, engine)
    }

    pub fn from_dir(root: &Path, engine: &Engine) -> Result<Self, String> {
        let meta = std::fs::metadata(root).map_err(|e| {
            format!(
                "Không thể truy cập thư mục plugin '{}': {e}",
                root.display()
            )
        })?;
        if !meta.is_dir() {
            return Err(format!(
                "Đường dẫn '{}' không phải thư mục plugin hợp lệ.",
                root.display()
            ));
        }

        let mut manifest_content: Option<String> = None;
        let mut wasm_content: Option<Vec<u8>> = None;
        let mut js_content: Option<Vec<u8>> = None;
        let mut block_files = BTreeMap::new();
        let mut regular_files = 0usize;
        let mut total_actual_uncompressed = 0usize;
        let mut seen_lower = HashSet::new();

        walk_plugin_dir(
            root,
            "",
            0,
            &mut regular_files,
            &mut total_actual_uncompressed,
            &mut seen_lower,
            &mut manifest_content,
            &mut wasm_content,
            &mut js_content,
            &mut block_files,
        )?;

        let manifest_text = manifest_content
            .ok_or_else(|| "Thư mục plugin thiếu file manifest.json.".to_owned())?;

        Self::finish_from_raw_parts(manifest_text, wasm_content, js_content, block_files, engine)
            .map_err(str::to_owned)
    }

    pub fn from_path(path: &Path, engine: &Engine) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| {
            format!(
                "Không thể truy cập đường dẫn plugin '{}': {e}",
                path.display()
            )
        })?;
        if meta.is_file() {
            if meta.len() > MAX_PLUGIN_ZIP_BYTES as u64 {
                return Err("Kích thước file ZIP vượt quá giới hạn 5 MB.".to_owned());
            }
            let file = std::fs::File::open(path)
                .map_err(|e| format!("Không thể mở file ZIP '{}': {e}", path.display()))?;
            let mut bytes = Vec::with_capacity(meta.len() as usize);
            file.take((MAX_PLUGIN_ZIP_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Không thể đọc file ZIP '{}': {e}", path.display()))?;
            Self::from_zip(&bytes, engine).map_err(str::to_owned)
        } else if meta.is_dir() {
            Self::from_dir(path, engine)
        } else {
            Err(format!(
                "Đường dẫn '{}' không phải file hoặc thư mục hợp lệ.",
                path.display()
            ))
        }
    }

    fn finish_from_raw_parts(
        manifest_text: String,
        wasm_content: Option<Vec<u8>>,
        js_content: Option<Vec<u8>>,
        block_files: BTreeMap<String, Vec<u8>>,
        engine: &Engine,
    ) -> Result<Self, &'static str> {
        let manifest = PluginManifest::from_json_str(&manifest_text)?;
        if wasm_content.is_some() && js_content.is_some() {
            return Err("Gói plugin không được chứa đồng thời cả plugin.wasm và plugin.js.");
        }
        let blocks = crate::theme::collect_package_blocks(&block_files, &manifest.id)?;
        let raw: serde_json::Value = serde_json::from_str(&manifest_text)
            .map_err(|_| "admin_extensions không đúng JSON.")?;
        let admin_extensions = match raw.get("admin_extensions") {
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|_| "admin_extensions không đúng cấu trúc.")?,
            ),
            None => None,
        };

        let wasm_bytes = match manifest.entrypoint.as_str() {
            "plugin.js" => {
                if wasm_content.is_some() {
                    return Err(
                        "Manifest khai báo entrypoint 'plugin.js' nhưng gói chứa plugin.wasm.",
                    );
                }
                let bytes = js_content.ok_or("Gói plugin thiếu file plugin.js.")?;
                let src = std::str::from_utf8(&bytes)
                    .map_err(|_| "File plugin.js phải là UTF-8 hợp lệ.")?;
                super::js_runtime::validate_js_module_syntax(src)
                    .map_err(|_| "File plugin.js có lỗi cú pháp JavaScript.")?;
                bytes
            }
            _ => {
                if js_content.is_some() {
                    return Err(
                        "Manifest khai báo entrypoint 'plugin.wasm' nhưng gói chứa plugin.js.",
                    );
                }
                let bytes = wasm_content.ok_or("Gói plugin thiếu file plugin.wasm.")?;
                validate_wasm_module_bytes(engine, &bytes)?;
                bytes
            }
        };

        Ok(Self {
            manifest,
            admin_extensions,
            wasm_bytes,
            blocks,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_plugin_dir(
    dir: &Path,
    rel_prefix: &str,
    depth: usize,
    regular_files: &mut usize,
    total_actual_uncompressed: &mut usize,
    seen_lower: &mut HashSet<String>,
    manifest_content: &mut Option<String>,
    wasm_content: &mut Option<Vec<u8>>,
    js_content: &mut Option<Vec<u8>>,
    block_files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Không thể đọc thư mục '{}': {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Lỗi đọc mục thư mục plugin: {e}"))?;
        let file_name = entry.file_name();
        let name = file_name.to_str().ok_or_else(|| {
            "Tên file hoặc thư mục trong plugin không phải UTF-8 hợp lệ.".to_owned()
        })?;
        if name.starts_with('.') {
            return Err(format!("Không cho phép entry ẩn trong plugin: {name}"));
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
                "Không cho phép symlink bên trong plugin: {rel_path}"
            ));
        }
        if ft.is_dir() {
            if depth >= MAX_PLUGIN_DIR_DEPTH {
                return Err(format!(
                    "Thư mục plugin vượt quá giới hạn độ sâu tối đa ({MAX_PLUGIN_DIR_DEPTH}): {rel_path}"
                ));
            }
            validate_theme_relative_path(&rel_path).map_err(str::to_owned)?;
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn plugin bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            walk_plugin_dir(
                &entry_path,
                &rel_path,
                depth + 1,
                regular_files,
                total_actual_uncompressed,
                seen_lower,
                manifest_content,
                wasm_content,
                js_content,
                block_files,
            )?;
        } else if ft.is_file() {
            let rel_path = validate_theme_relative_path(&rel_path).map_err(str::to_owned)?;
            *regular_files += 1;
            if *regular_files > MAX_PLUGIN_FILES {
                return Err(format!(
                    "Thư mục plugin chứa quá nhiều file (tối đa {MAX_PLUGIN_FILES} file)."
                ));
            }
            if !seen_lower.insert(rel_path.to_ascii_lowercase()) {
                return Err(format!(
                    "Đường dẫn plugin bị trùng lặp không phân biệt hoa thường: {rel_path}"
                ));
            }
            let max_file_limit = plugin_entry_max_bytes(&rel_path).ok_or_else(|| {
                format!("Thư mục plugin chứa file không được hỗ trợ trong hợp đồng ABI: {rel_path}")
            })?;
            let remaining_total =
                MAX_UNCOMPRESSED_TOTAL_BYTES.saturating_sub(*total_actual_uncompressed);
            let read_cap = std::cmp::min(max_file_limit, remaining_total);
            if meta.len() > read_cap as u64 {
                return Err(format!(
                    "File '{rel_path}' vượt quá giới hạn kích thước ({read_cap} bytes)."
                ));
            }
            let file = std::fs::File::open(&entry_path)
                .map_err(|e| format!("Không thể mở file '{rel_path}': {e}"))?;
            let mut buffer = Vec::with_capacity((meta.len() as usize).min(read_cap));
            file.take((read_cap + 1) as u64)
                .read_to_end(&mut buffer)
                .map_err(|e| format!("Không thể đọc file '{rel_path}': {e}"))?;
            if buffer.len() > max_file_limit {
                return Err(match rel_path.as_str() {
                    "manifest.json" => {
                        "manifest.json vượt quá giới hạn kích thước 64 KB.".to_owned()
                    }
                    "plugin.wasm" => {
                        "File plugin.wasm vượt quá giới hạn kích thước 2 MB.".to_owned()
                    }
                    "plugin.js" => "File plugin.js vượt quá giới hạn kích thước 2 MB.".to_owned(),
                    _ => format!("File '{rel_path}' vượt quá giới hạn kích thước."),
                });
            }
            *total_actual_uncompressed = total_actual_uncompressed
                .checked_add(buffer.len())
                .ok_or_else(|| "Tổng dung lượng plugin vượt quá giới hạn.".to_owned())?;
            if *total_actual_uncompressed > MAX_UNCOMPRESSED_TOTAL_BYTES {
                return Err("Tổng dung lượng plugin vượt quá giới hạn 10 MB.".to_owned());
            }
            match rel_path.as_str() {
                "manifest.json" => {
                    let text = String::from_utf8(buffer)
                        .map_err(|_| "manifest.json không phải UTF-8 hợp lệ.".to_owned())?;
                    *manifest_content = Some(text);
                }
                "plugin.wasm" => {
                    *wasm_content = Some(buffer);
                }
                "plugin.js" => {
                    *js_content = Some(buffer);
                }
                _ if rel_path.starts_with("blocks/") => {
                    block_files.insert(rel_path, buffer);
                }
                _ => {}
            }
        } else {
            return Err(format!(
                "Không cho phép entry không phải file thường trong plugin: {rel_path}"
            ));
        }
    }
    Ok(())
}

pub fn check_plugin_path(path: &Path) -> Result<PluginCheckSummary, String> {
    let mut config = wasmi::Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let pkg = PluginPackage::from_path(path, &engine)?;
    let mut hooks: Vec<String> = pkg
        .manifest
        .hooks
        .iter()
        .map(|h| h.as_str().to_owned())
        .collect();
    hooks.sort();

    let mut filters: Vec<String> = pkg
        .manifest
        .filters
        .iter()
        .map(|f| f.as_str().to_owned())
        .collect();
    filters.sort();

    let mut actions: Vec<String> = pkg.manifest.actions.iter().map(|a| a.id.clone()).collect();
    actions.sort();

    let mut blocks: Vec<String> = pkg.blocks.iter().map(|(s, _)| s.id.clone()).collect();
    blocks.sort();

    Ok(PluginCheckSummary {
        ok: true,
        plugin_id: pkg.manifest.id,
        version: pkg.manifest.version,
        manifest_schema: pkg.manifest.manifest_schema,
        hooks,
        filters,
        actions,
        blocks,
        settings_count: pkg
            .manifest
            .settings_schema
            .iter()
            .filter(|s| !s.is_ui_only())
            .count(),
    })
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod block_package_tests;
