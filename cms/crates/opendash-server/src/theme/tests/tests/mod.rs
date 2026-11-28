use super::*;
use std::io::{Read, Write};
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

/// D01 test fixture: all ZIP entries, including empty directory entries,
/// pass through the production bounded reader before manifest validation.
fn build_theme_zip(manifest: serde_json::Value, entries: &[(&str, &[u8])]) -> Vec<u8> {
    let manifest_json = serde_json::to_vec(&manifest).unwrap();
    let mut buf = Vec::new();
    {
        let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zw.start_file("theme.json", opts).unwrap();
        zw.write_all(&manifest_json).unwrap();
        for (path, bytes) in entries {
            if path.ends_with('/') {
                zw.add_directory(*path, opts).unwrap();
            } else {
                zw.start_file(*path, opts).unwrap();
                zw.write_all(bytes).unwrap();
            }
        }
        zw.finish().unwrap();
    }
    buf
}

fn minimal_theme_manifest() -> serde_json::Value {
    serde_json::json!({
        "manifest_version": 1,
        "theme_id": "sample-theme",
        "name": "Sample",
        "version": "1.0.0"
    })
}

mod suite_01;
mod suite_02;
