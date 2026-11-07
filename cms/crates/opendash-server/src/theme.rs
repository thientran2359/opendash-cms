use minijinja::{AutoEscape, Environment, UndefinedBehavior};
pub use opendash_core::theme::{Block, BlockDescriptor, Fields};

use crate::package::{ensure_safe_flat_entry, open_bounded_zip, read_bounded_entry};

pub const MAX_ZIP_BYTES: usize = 64 * 1024;

pub struct Theme {
    pub descriptor: BlockDescriptor,
    pub template: String,
}

impl Theme {
    pub fn from_zip(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut archive = open_bounded_zip(
            bytes,
            MAX_ZIP_BYTES,
            "ZIP vượt quá giới hạn kích thước.",
            "ZIP không hợp lệ.",
        )?;
        if archive.len() != 2 {
            return Err("ZIP phải chứa block.json và block.html.");
        }

        let mut descriptor = None;
        let mut template = None;
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

        let template = template.ok_or("Thiếu block.html.")?;
        if template.trim().is_empty() || environment().template_from_str(&template).is_err() {
            return Err("Template block không hợp lệ.");
        }
        Ok(Self {
            descriptor,
            template,
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

fn environment() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::Html);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_fuel(Some(10_000));
    env
}
