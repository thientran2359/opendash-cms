use opendash_core::block_schema::BlockSchemaV1;
use std::collections::{BTreeMap, HashSet};
use wasmi::{Engine, ExternType, Module, ValType};

use crate::package::{
    V2_MAX_EXPANSION_RATIO, open_bounded_zip, read_bounded_entry, validate_v2_relative_path,
};

use super::manifest::PluginManifest;

pub const MAX_PLUGIN_ZIP_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_UNCOMPRESSED_TOTAL_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_WASM_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024; // 64 KB
pub const MAX_PLUGIN_FILES: usize = 72;

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
                    validate_v2_relative_path(trimmed)?;
                }
                if file.size() != 0 {
                    return Err("Thư mục trong ZIP phải rỗng.");
                }
                continue;
            }
            let name = validate_v2_relative_path(&name)?;
            regular_files += 1;
            if regular_files > MAX_PLUGIN_FILES {
                return Err("ZIP chứa quá nhiều file (tối đa 72 file).");
            }
            if !seen_lower.insert(name.to_ascii_lowercase()) {
                return Err("ZIP chứa mục trùng lặp hoặc xung đột chữ hoa/thường.");
            }

            // Only contract-supported files allowed
            let is_block_file = name.starts_with("blocks/");
            if name != "manifest.json"
                && name != "plugin.wasm"
                && name != "README.md"
                && !is_block_file
            {
                return Err("ZIP chứa file không được hỗ trợ trong hợp đồng ABI v1.");
            }

            let max_file_limit = match name.as_str() {
                "manifest.json" => MAX_MANIFEST_BYTES,
                "plugin.wasm" => MAX_WASM_BYTES,
                _ if is_block_file => crate::package::V2_MAX_TEXT_BYTES,
                _ => MAX_UNCOMPRESSED_TOTAL_BYTES,
            };

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
                _ if is_block_file => {
                    block_files.insert(name, buffer);
                }
                _ => {
                    // Allowed optional files like README.md
                }
            }
        }

        let manifest_text = manifest_content.ok_or("ZIP thiếu file manifest.json.")?;
        let wasm_bytes = wasm_content.ok_or("ZIP thiếu file plugin.wasm.")?;

        let manifest = PluginManifest::from_json_str(&manifest_text)?;
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

        // Validate Wasm module structure using Wasmi
        let module = Module::new(engine, &wasm_bytes)
            .map_err(|_| "Module WebAssembly không hợp lệ hoặc cấu trúc bị lỗi.")?;

        // Verify required exports
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

        // Verify imports: only allowed exact signatures from "opendash_v1" namespace or "env:abort"
        for import in module.imports() {
            let mod_name = import.module();
            let name = import.name();
            match (mod_name, name) {
                ("opendash_v1", "host_get_input_len") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if !func_ty.params().is_empty() || func_ty.results() != [ValType::I32] {
                            return Err("Import 'opendash_v1:host_get_input_len' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_get_input_len' phải là hàm.");
                    }
                }
                ("opendash_v1", "host_read_input") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if func_ty.params() != [ValType::I32, ValType::I32]
                            || func_ty.results() != [ValType::I32]
                        {
                            return Err("Import 'opendash_v1:host_read_input' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_read_input' phải là hàm.");
                    }
                }
                ("opendash_v1", "host_write_output") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if func_ty.params() != [ValType::I32, ValType::I32]
                            || func_ty.results() != [ValType::I32]
                        {
                            return Err("Import 'opendash_v1:host_write_output' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_write_output' phải là hàm.");
                    }
                }
                ("opendash_v1", "host_call") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if func_ty.params()
                            != [ValType::I32, ValType::I32, ValType::I32, ValType::I32]
                            || func_ty.results() != [ValType::I32]
                        {
                            return Err("Import 'opendash_v1:host_call' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_call' phải là hàm.");
                    }
                }
                ("opendash_v1", "host_last_resp_len") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if !func_ty.params().is_empty() || func_ty.results() != [ValType::I32] {
                            return Err("Import 'opendash_v1:host_last_resp_len' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_last_resp_len' phải là hàm.");
                    }
                }
                ("opendash_v1", "host_read_last_resp") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if func_ty.params() != [ValType::I32, ValType::I32]
                            || func_ty.results() != [ValType::I32]
                        {
                            return Err("Import 'opendash_v1:host_read_last_resp' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'opendash_v1:host_read_last_resp' phải là hàm.");
                    }
                }
                ("env", "abort") => {
                    if let ExternType::Func(func_ty) = import.ty() {
                        if func_ty.params()
                            != [ValType::I32, ValType::I32, ValType::I32, ValType::I32]
                            || !func_ty.results().is_empty()
                        {
                            return Err("Import 'env:abort' sai chữ ký hàm.");
                        }
                    } else {
                        return Err("Import 'env:abort' phải là hàm.");
                    }
                }
                _ => {
                    return Err(
                        "Module WebAssembly chứa import không được phép (từ chối WASI/unknown env).",
                    );
                }
            }
        }

        Ok(Self {
            manifest,
            admin_extensions,
            wasm_bytes,
            blocks,
        })
    }
}

#[cfg(test)]
mod block_package_tests {
    use super::*;
    use std::io::Write;
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn make_zip(entries: &[(String, Vec<u8>)], with_directories: bool) -> Vec<u8> {
        let mut buffer = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buffer));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let manifest = br#"{"id":"sample-plugin","version":"1.0.0","api_version":1,"name":"Sample","entrypoint":"plugin.wasm","permissions":{}}"#;
            let wasm = wat::parse_str("(module (memory (export \"memory\") 1) (func (export \"opendash_run\") (result i32) (i32.const 0)))").unwrap();
            zip.start_file("manifest.json", opts).unwrap();
            zip.write_all(manifest).unwrap();
            zip.start_file("plugin.wasm", opts).unwrap();
            zip.write_all(&wasm).unwrap();
            if with_directories {
                zip.add_directory("blocks/", opts).unwrap();
                zip.add_directory("blocks/notice/", opts).unwrap();
            }
            for (name, bytes) in entries {
                zip.start_file(name.as_str(), opts).unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        buffer
    }

    fn schema(slug: &str, owner: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version":1, "id":format!("{owner}/{slug}"),
            "title":"Notice", "category":"content", "render":"render.html",
            "fields": []
        }))
        .unwrap()
    }

    fn pair(slug: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                format!("blocks/{slug}/block.json"),
                schema(slug, "sample-plugin"),
            ),
            (
                format!("blocks/{slug}/render.html"),
                b"<p>Safe</p>".to_vec(),
            ),
        ]
    }

    #[test]
    fn zip_accepts_paired_blocks_and_empty_directory_entries() {
        let package = PluginPackage::from_zip(
            &make_zip(&pair("notice"), true),
            crate::plugin::runtime::PluginRuntime::new().engine(),
        )
        .expect("paired plugin block");
        assert_eq!(package.blocks.len(), 1);
        assert_eq!(package.blocks[0].0.id, "sample-plugin/notice");
    }

    #[test]
    fn zip_rejects_missing_pair_case_collisions_reserved_and_unsafe_templates() {
        let engine = crate::plugin::runtime::PluginRuntime::new();
        let mut missing_pair = pair("notice");
        missing_pair.pop();
        assert!(PluginPackage::from_zip(&make_zip(&missing_pair, false), engine.engine()).is_err());

        let mut case_collision = pair("notice");
        case_collision.push((
            "blocks/Notice/block.json".into(),
            schema("notice", "sample-plugin"),
        ));
        assert!(
            PluginPackage::from_zip(&make_zip(&case_collision, false), engine.engine()).is_err()
        );

        let mut wrong_owner = pair("notice");
        wrong_owner[0].1 = schema("notice", "core");
        assert!(PluginPackage::from_zip(&make_zip(&wrong_owner, false), engine.engine()).is_err());

        let mut unsafe_html = pair("notice");
        unsafe_html[1].1 = b"<script>alert(1)</script>".to_vec();
        assert!(PluginPackage::from_zip(&make_zip(&unsafe_html, false), engine.engine()).is_err());

        let mut traversal = pair("notice");
        traversal[1].0 = "blocks/../evil.html".into();
        assert!(PluginPackage::from_zip(&make_zip(&traversal, false), engine.engine()).is_err());
    }

    #[test]
    fn zip_rejects_too_many_files_and_blocks() {
        let engine = crate::plugin::runtime::PluginRuntime::new();
        let mut entries = Vec::new();
        for i in 0..36 {
            entries.extend(pair(&format!("notice-{i}")));
        }
        assert!(PluginPackage::from_zip(&make_zip(&entries, false), engine.engine()).is_err());
        entries.extend(pair("notice-extra"));
        assert!(PluginPackage::from_zip(&make_zip(&entries, false), engine.engine()).is_err());
    }
}
