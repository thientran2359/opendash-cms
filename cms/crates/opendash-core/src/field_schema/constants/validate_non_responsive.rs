use super::*;
#[allow(clippy::too_many_arguments)]

pub(crate) fn validate_non_responsive_value(
    spec: &FieldSpec,
    ctx: FieldContext,
    val: &Value,
    visible: bool,
    prev_visible: bool,
    upgrade_exempt: bool,
    prev_val: Option<&Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    in_root: bool,
) -> Result<(), &'static str> {
    let req = visible && !upgrade_exempt && spec.effective_required();
    match spec.field_type {
        FieldType::Text => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị text phải là chuỗi.");
            };
            if s.chars().any(char::is_control) {
                return Err("Giá trị text không được chứa ký tự điều khiển.");
            }
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXT_MAX_CHARS);
            let len = s.chars().count();
            if len > max_c {
                return Err("Giá trị text vượt quá max_chars.");
            }
            if req && s.trim().is_empty() {
                return Err("Trường text bắt buộc không được để trống.");
            }
            if !s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn) {
                return Err("Giá trị text ngắn hơn min_chars.");
            }
        }
        FieldType::Textarea => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị textarea phải là chuỗi.");
            };
            let max_c = spec.max_chars.unwrap_or(DEFAULT_TEXTAREA_MAX_CHARS);
            let len = s.chars().count();
            if len > max_c {
                return Err("Giá trị textarea vượt quá max_chars.");
            }
            if req && s.trim().is_empty() {
                return Err("Trường textarea bắt buộc không được để trống.");
            }
            if !s.is_empty() && spec.min_chars.is_some_and(|mn| len < mn) {
                return Err("Giá trị textarea ngắn hơn min_chars.");
            }
        }
        FieldType::Number | FieldType::Range => {
            let Some(n) = val.as_f64().filter(|f| f.is_finite()) else {
                return Err("Giá trị number/range phải là số hữu hạn.");
            };
            if spec.min.is_some_and(|mn| n < mn) || spec.max.is_some_and(|mx| n > mx) {
                return Err("Giá trị number/range nằm ngoài [min, max].");
            }
        }
        FieldType::Email => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị email phải là chuỗi.");
            };
            if s.trim().is_empty() {
                if req {
                    return Err("Trường email bắt buộc không được để trống.");
                }
            } else {
                let max_c = spec.max_chars.unwrap_or(MAX_EMAIL_CHARS);
                if s.chars().count() > max_c || !valid_email_address(s) {
                    return Err("Địa chỉ email không hợp lệ.");
                }
            }
        }
        FieldType::Url => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị url phải là chuỗi.");
            };
            if s.trim().is_empty() {
                if req {
                    return Err("Trường url bắt buộc không được để trống.");
                }
            } else if !valid_safe_url(s) {
                return Err("URL không hợp lệ hoặc dùng scheme không an toàn.");
            }
        }
        FieldType::Secret => {
            // Wire representations allow write-only updates or null/sentinel placeholders;
            // exact SecretStore handling occurs in storage/settings.rs.
        }
        FieldType::Richtext => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị richtext phải là chuỗi.");
            };
            if req && strip_richtext_to_plain(s).trim().is_empty() {
                return Err("Trường richtext bắt buộc không được để trống.");
            }
            if ctx == FieldContext::Content && in_root && spec.key == "body" {
                if contains_blocks(s) {
                    if s.len() > MAX_BUILDER_BLOCK_BODY_BYTES {
                        return Err("Nội dung block body vượt giới hạn 384 KiB.");
                    }
                } else if s.chars().count() > 200_000 {
                    return Err("Nội dung body vượt giới hạn 200000 ký tự.");
                }
            } else if !s.is_empty() {
                let cleaned = sanitize_richtext(s)?;
                if req && strip_richtext_to_plain(&cleaned).trim().is_empty() {
                    return Err("Trường richtext bắt buộc không được để trống.");
                }
                let cap = spec.max_bytes.unwrap_or(MAX_RICHTEXT_OUTPUT_BYTES);
                if cleaned.len() > cap {
                    return Err("Nội dung richtext sau làm sạch vượt quá max_bytes.");
                }
            }
        }
        FieldType::Media => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị media phải là chuỗi media_id.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường media bắt buộc chưa chọn tệp.");
                }
            } else if !valid_sha256_media_id(s) {
                return Err("media_id phải là chuỗi SHA-256 64 ký tự hex.");
            }
        }
        FieldType::Gallery => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị gallery phải là mảng media_id.");
            };
            let max_i = spec.max_items.unwrap_or(MAX_GALLERY_ITEMS);
            if arr.len() > max_i {
                return Err("Số lượng phần tử gallery vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường gallery bắt buộc không được để trống.");
            }
            if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số lượng phần tử gallery ít hơn min_items.");
            }
            let mut seen = BTreeSet::new();
            for item in arr {
                let Some(id) = item.as_str() else {
                    return Err("Phần tử gallery phải là chuỗi media_id.");
                };
                if !valid_sha256_media_id(id) || !seen.insert(id) {
                    return Err("media_id trong gallery không hợp lệ hoặc bị trùng.");
                }
            }
        }
        FieldType::Link => {
            let link: LinkValue = serde_json::from_value(val.clone())
                .map_err(|_| "Giá trị link không đúng cấu trúc.")?;
            if link.label.chars().count() > MAX_LINK_LABEL_CHARS
                || link.label.chars().any(char::is_control)
            {
                return Err("Nhãn link tối đa 160 ký tự và không chứa ký tự điều khiển.");
            }
            if link.url.is_empty() {
                if req {
                    return Err("Trường link bắt buộc chưa có URL.");
                }
            } else if !valid_safe_url(&link.url) {
                return Err("URL trong field link không hợp lệ.");
            }
        }
        FieldType::Select => {
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            if spec.multiple {
                let Some(arr) = val.as_array() else {
                    return Err("Giá trị select multiple phải là mảng.");
                };
                if req && arr.is_empty() {
                    return Err("Trường select bắt buộc phải chọn ít nhất 1 mục.");
                }
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(s) = item.as_str() else {
                        return Err("Phần tử select multiple phải là chuỗi.");
                    };
                    if !seen.insert(s) {
                        return Err("Giá trị select multiple không được trùng.");
                    }
                    if spec.options_source.is_none() && !opt_vals.contains(s) {
                        return Err("Lựa chọn không có trong danh sách options.");
                    }
                }
            } else {
                let Some(s) = val.as_str() else {
                    return Err("Giá trị select phải là chuỗi.");
                };
                if s.is_empty() {
                    if req {
                        return Err("Trường select bắt buộc chưa được chọn.");
                    }
                } else if spec.options_source.is_none() && !opt_vals.contains(s) {
                    return Err("Lựa chọn không có trong danh sách options.");
                }
            }
        }
        FieldType::Checkbox => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị checkbox phải là mảng.");
            };
            let max_i = spec.max_items.unwrap_or(spec.options.len());
            if arr.len() > max_i {
                return Err("Số lượng checkbox vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường checkbox bắt buộc phải chọn ít nhất 1 mục.");
            }
            if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số lượng checkbox ít hơn min_items.");
            }
            let opt_vals: BTreeSet<&str> = spec.options.iter().map(|o| o.value.as_str()).collect();
            let mut seen = BTreeSet::new();
            for item in arr {
                let Some(s) = item.as_str() else {
                    return Err("Phần tử checkbox phải là chuỗi.");
                };
                if !opt_vals.contains(s) || !seen.insert(s) {
                    return Err("Giá trị checkbox không hợp lệ hoặc bị trùng.");
                }
            }
        }
        FieldType::RadioButtons => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị radio_buttons phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường radio_buttons bắt buộc chưa được chọn.");
                }
            } else if !spec.options.iter().any(|o| o.value == s) {
                return Err("Giá trị radio_buttons không có trong options.");
            }
        }
        FieldType::Boolean => {
            if !val.is_boolean() {
                return Err("Giá trị boolean phải là true hoặc false.");
            }
        }
        FieldType::Color => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị color phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường color bắt buộc chưa chọn màu.");
                }
            } else if !valid_color_value(s) {
                return Err("Mã màu không hợp lệ.");
            }
        }
        FieldType::Icon => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị icon phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường icon bắt buộc chưa chọn biểu tượng.");
                }
            } else if lookup_core_icon_paths(s).is_none() {
                return Err("Tên icon không hợp lệ trong CORE_ICONS.");
            }
        }
        FieldType::Date => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị date phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường date bắt buộc chưa có ngày.");
                }
            } else {
                if !valid_iso_date(s) {
                    return Err("Ngày phải có định dạng YYYY-MM-DD hợp lệ.");
                }
                if spec.min_date.as_deref().is_some_and(|mn| s < mn)
                    || spec.max_date.as_deref().is_some_and(|mx| s > mx)
                {
                    return Err("Ngày nằm ngoài khoảng min_date..=max_date.");
                }
            }
        }
        FieldType::Datetime => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị datetime phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường datetime bắt buộc chưa có thời gian.");
                }
            } else if !valid_iso_datetime_utc(s) {
                return Err("datetime phải là ISO-8601 UTC hợp lệ.");
            }
        }
        FieldType::Time => {
            let Some(s) = val.as_str() else {
                return Err("Giá trị time phải là chuỗi.");
            };
            if s.is_empty() {
                if req {
                    return Err("Trường time bắt buộc chưa có giờ.");
                }
            } else if !valid_iso_time(s) {
                return Err("time phải có định dạng HH:MM hoặc HH:MM:SS.");
            }
        }
        FieldType::Spacing => {
            let sp: SpacingValue = serde_json::from_value(val.clone())
                .map_err(|_| "Giá trị spacing không đúng cấu trúc.")?;
            let prop = spec.effective_spacing_property();
            let lo = spec.min.unwrap_or(match prop {
                SpacingProperty::Padding => 0.0,
                SpacingProperty::Margin => -128.0,
            });
            let hi = spec.max.unwrap_or(256.0);
            let sides = sp.sides();
            if req && sides.iter().all(Option::is_none) {
                return Err("Trường spacing bắt buộc phải có ít nhất 1 cạnh.");
            }
            for s in sides.into_iter().flatten() {
                if !s.is_finite() || s < lo || s > hi {
                    return Err("Giá trị spacing nằm ngoài [min, max].");
                }
            }
        }
        FieldType::EntryRef | FieldType::TermRef => {
            let default_max = if spec.field_type == FieldType::EntryRef {
                MAX_ENTRY_REF_ITEMS
            } else {
                MAX_TERM_REF_ITEMS
            };
            if spec.multiple {
                let Some(arr) = val.as_array() else {
                    return Err("Trường quan hệ multiple phải là mảng số nguyên.");
                };
                let max_i = spec.max_items.unwrap_or(default_max);
                if arr.len() > max_i {
                    return Err("Số lượng tham chiếu vượt max_items.");
                }
                if req && arr.is_empty() {
                    return Err("Trường quan hệ bắt buộc phải chọn ít nhất 1 mục.");
                }
                if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Số lượng tham chiếu ít hơn min_items.");
                }
                let mut seen = BTreeSet::new();
                for item in arr {
                    let Some(id) = item.as_i64().filter(|n| *n > 0) else {
                        return Err("ID tham chiếu phải là số nguyên dương.");
                    };
                    if !seen.insert(id) {
                        return Err("ID tham chiếu không được trùng lặp.");
                    }
                }
            } else if val.as_i64().is_none_or(|n| n <= 0) {
                return Err("ID tham chiếu đơn phải là số nguyên dương.");
            }
        }
        FieldType::Group => {
            let Some(obj) = val.as_object() else {
                return Err("Giá trị group phải là JSON object.");
            };
            let map: BTreeMap<String, Value> =
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let prev_map: Option<BTreeMap<String, Value>> = prev_val
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            validate_map_against_specs(
                &spec.fields,
                ctx,
                &map,
                prev_map.as_ref(),
                root_specs,
                root_values,
                prev_root_values,
                visible,
                prev_visible,
            )?;
        }
        FieldType::Repeater => {
            let Some(arr) = val.as_array() else {
                return Err("Giá trị repeater phải là mảng.");
            };
            let cap = if ctx == FieldContext::StudioBlock {
                MAX_BLOCK_REPEATER_ITEMS
            } else {
                MAX_REPEATER_ITEMS
            };
            let max_items = spec.max_items.unwrap_or(cap);
            if arr.len() > max_items {
                return Err("Số phần tử repeater vượt quá max_items.");
            }
            if req && arr.is_empty() {
                return Err("Trường repeater bắt buộc phải có ít nhất 1 phần tử.");
            }
            if ctx == FieldContext::StudioBlock {
                if visible && !upgrade_exempt && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                    return Err("Số phần tử repeater ít hơn min_items.");
                }
            } else if !arr.is_empty() && spec.min_items.is_some_and(|mn| arr.len() < mn) {
                return Err("Số phần tử repeater ít hơn min_items.");
            }
            for (idx, item) in arr.iter().enumerate() {
                let Some(obj) = item.as_object() else {
                    return Err("Phần tử repeater phải là JSON object.");
                };
                if ctx == FieldContext::StudioBlock
                    && obj.len() > crate::block_schema::MAX_NODE_VALUES_KEYS
                {
                    return Err("Phần tử Repeater vượt quá 64 khóa.");
                }
                let map: BTreeMap<String, Value> =
                    obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                let prev_map: Option<BTreeMap<String, Value>> = prev_val
                    .and_then(Value::as_array)
                    .and_then(|rows| rows.get(idx))
                    .and_then(Value::as_object)
                    .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                validate_map_against_specs(
                    &spec.fields,
                    ctx,
                    &map,
                    prev_map.as_ref(),
                    root_specs,
                    root_values,
                    prev_root_values,
                    visible,
                    prev_visible,
                )?;
            }
        }
        FieldType::Tab | FieldType::Accordion | FieldType::Message => {}
    }
    Ok(())
}
