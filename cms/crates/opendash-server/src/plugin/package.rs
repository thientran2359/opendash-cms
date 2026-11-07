use wasmi::{Engine, ExternType, Module, ValType};

use crate::package::{ensure_safe_flat_entry, open_bounded_zip, read_bounded_entry};

use super::manifest::PluginManifest;

pub const MAX_PLUGIN_ZIP_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_UNCOMPRESSED_TOTAL_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_WASM_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024; // 64 KB
pub const MAX_PLUGIN_FILES: usize = 10;

#[derive(Debug)]
pub struct PluginPackage {
    pub manifest: PluginManifest,
    pub wasm_bytes: Vec<u8>,
}

impl PluginPackage {
    pub fn from_zip(bytes: &[u8], engine: &Engine) -> Result<Self, &'static str> {
        let mut archive = open_bounded_zip(
            bytes,
            MAX_PLUGIN_ZIP_BYTES,
            "Kích thước file ZIP vượt quá giới hạn 5 MB.",
            "ZIP không hợp lệ.",
        )?;
        if archive.len() > MAX_PLUGIN_FILES {
            return Err("ZIP chứa quá nhiều file (tối đa 10 file).");
        }

        let mut manifest_content: Option<String> = None;
        let mut wasm_content: Option<Vec<u8>> = None;
        let mut total_actual_uncompressed: usize = 0;
        let mut seen_names = std::collections::BTreeSet::new();

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|_| "Không thể đọc mục trong ZIP.")?;
            let name = ensure_safe_flat_entry(
                &file,
                "ZIP chứa đường dẫn không an toàn hoặc thư mục con.",
                "ZIP chứa symlink bị cấm.",
                "ZIP không được chứa thư mục con.",
            )?;
            if !seen_names.insert(name.clone()) {
                return Err("ZIP chứa mục trùng lặp.");
            }

            // Only contract-supported files allowed
            if name != "manifest.json" && name != "plugin.wasm" && name != "README.md" {
                return Err("ZIP chứa file không được hỗ trợ trong hợp đồng ABI v1.");
            }

            let max_file_limit = match name.as_str() {
                "manifest.json" => MAX_MANIFEST_BYTES,
                "plugin.wasm" => MAX_WASM_BYTES,
                _ => MAX_UNCOMPRESSED_TOTAL_BYTES,
            };

            let remaining_total =
                MAX_UNCOMPRESSED_TOTAL_BYTES.saturating_sub(total_actual_uncompressed);
            let read_cap = std::cmp::min(max_file_limit, remaining_total);

            let buffer =
                read_bounded_entry(&mut file, read_cap, "Lỗi khi đọc nội dung file trong ZIP.")?;

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
                _ => {
                    // Allowed optional files like README.md
                }
            }
        }

        let manifest_text = manifest_content.ok_or("ZIP thiếu file manifest.json.")?;
        let wasm_bytes = wasm_content.ok_or("ZIP thiếu file plugin.wasm.")?;

        let manifest = PluginManifest::from_json_str(&manifest_text)?;

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
            wasm_bytes,
        })
    }
}
