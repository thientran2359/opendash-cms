use super::*;

pub fn validate_field_values(
    specs: &[FieldSpec],
    ctx: FieldContext,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
) -> Result<(), &'static str> {
    validate_map_against_specs(
        specs,
        ctx,
        values,
        previous_values,
        specs,
        values,
        previous_values,
        true,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_map_against_specs(
    specs: &[FieldSpec],
    ctx: FieldContext,
    values: &BTreeMap<String, Value>,
    previous_values: Option<&BTreeMap<String, Value>>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
    prev_root_values: Option<&BTreeMap<String, Value>>,
    parent_visible: bool,
    parent_prev_visible: bool,
) -> Result<(), &'static str> {
    let in_root =
        std::ptr::eq(specs.as_ptr(), root_specs.as_ptr()) && std::ptr::eq(values, root_values);
    let data_specs: BTreeMap<&str, &FieldSpec> = specs
        .iter()
        .filter(|s| !s.is_ui_only())
        .map(|s| (s.key.as_str(), s))
        .collect();

    for (key, val) in values {
        if !data_specs.contains_key(key.as_str()) {
            if ctx == FieldContext::StudioBlock
                && previous_values
                    .and_then(|p| p.get(key.as_str()))
                    .is_some_and(|pv| value_equals_previous(Some(val), Some(pv)))
            {
                continue;
            }
            return Err("Payload chứa khóa field không tồn tại trong schema.");
        }
    }

    for spec in specs {
        if spec.is_ui_only() {
            continue;
        }
        let visible = parent_visible
            && evaluate_conditions(
                spec.conditions.as_ref(),
                specs,
                values,
                root_specs,
                root_values,
            );
        let prev_visible = parent_prev_visible
            && previous_values.is_some_and(|prev| {
                evaluate_conditions(
                    spec.conditions.as_ref(),
                    specs,
                    prev,
                    root_specs,
                    prev_root_values.unwrap_or(prev),
                )
            });
        let cur_val = values.get(spec.key.as_str());
        let prev_val = previous_values.and_then(|p| p.get(spec.key.as_str()));
        let upgrade_exempt = ctx == FieldContext::StudioBlock
            && previous_values
                .is_some_and(|prev| prev_visible && !prev.contains_key(spec.key.as_str()));

        let Some(raw_val) = cur_val.filter(|v| !v.is_null()) else {
            if visible && !upgrade_exempt && spec.field_type != FieldType::Secret {
                let missing_required = spec.effective_required()
                    || (ctx == FieldContext::StudioBlock
                        && spec.field_type == FieldType::Repeater
                        && spec.min_items.is_some_and(|m| m > 0));
                if missing_required {
                    return Err("Thiếu giá trị cho trường bắt buộc.");
                }
            }
            continue;
        };

        // Even when hidden, validate non-null values strictly against schema/XSS/bounds.
        validate_field_value(
            spec,
            ctx,
            raw_val,
            visible,
            prev_visible,
            upgrade_exempt,
            prev_val,
            root_specs,
            root_values,
            prev_root_values,
            in_root,
        )?;
    }

    Ok(())
}
