use super::*;

pub fn evaluate_conditions(
    conds: Option<&ConditionGroups>,
    scope_specs: &[FieldSpec],
    scope_values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
) -> bool {
    let Some(or_groups) = conds else {
        return true;
    };
    if or_groups.is_empty() {
        return true;
    }
    or_groups.iter().any(|and_rules| {
        and_rules.iter().all(|rule| {
            eval_single_condition(rule, scope_specs, scope_values, root_specs, root_values)
        })
    })
}

pub(crate) fn find_spec_in_slice<'a>(specs: &'a [FieldSpec], key: &str) -> Option<&'a FieldSpec> {
    specs.iter().find(|s| s.key == key)
}

pub(crate) fn resolve_effective_condition_value<'a>(
    key: &str,
    scope_specs: &'a [FieldSpec],
    scope_values: &'a BTreeMap<String, Value>,
    root_specs: &'a [FieldSpec],
    root_values: &'a BTreeMap<String, Value>,
) -> (Option<&'a FieldSpec>, Value) {
    if let Some(spec) = find_spec_in_slice(scope_specs, key) {
        let val = effective_value_for_spec(spec, scope_values.get(key));
        return (Some(spec), val);
    }
    if let Some(spec) = find_spec_in_slice(root_specs, key) {
        let val = effective_value_for_spec(spec, root_values.get(key));
        return (Some(spec), val);
    }
    (None, Value::Null)
}

pub(crate) fn effective_value_for_spec(spec: &FieldSpec, current: Option<&Value>) -> Value {
    match current {
        Some(v) if !v.is_null() => v.clone(),
        _ => spec
            .default
            .as_ref()
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or_else(|| canonical_empty_for_spec(spec)),
    }
}

pub fn canonical_empty_for_spec(spec: &FieldSpec) -> Value {
    match spec.field_type {
        FieldType::Text
        | FieldType::Textarea
        | FieldType::Email
        | FieldType::Url
        | FieldType::Richtext
        | FieldType::Color
        | FieldType::Icon
        | FieldType::Date
        | FieldType::Datetime
        | FieldType::Time
        | FieldType::RadioButtons => Value::String(String::new()),
        FieldType::Select => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else {
                Value::String(String::new())
            }
        }
        FieldType::EntryRef | FieldType::TermRef => {
            if spec.multiple {
                Value::Array(Vec::new())
            } else {
                Value::Null
            }
        }
        FieldType::Checkbox | FieldType::Gallery | FieldType::Repeater => Value::Array(Vec::new()),
        FieldType::Boolean => Value::Bool(false),
        FieldType::Group => Value::Object(serde_json::Map::new()),
        FieldType::Number
        | FieldType::Range
        | FieldType::Media
        | FieldType::Link
        | FieldType::Spacing
        | FieldType::Secret
        | FieldType::Tab
        | FieldType::Accordion
        | FieldType::Message => Value::Null,
    }
}

pub(crate) fn is_json_value_empty(val: &Value) -> bool {
    match val {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(arr) => arr.is_empty(),
        Value::Object(obj) => obj.is_empty(),
        Value::Number(_) | Value::Bool(_) => false,
    }
}

pub(crate) fn scalar_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            x.as_f64() == y.as_f64() && x.as_f64().is_some_and(f64::is_finite)
        }
        _ => false,
    }
}

pub(crate) fn eval_single_condition(
    rule: &ConditionRule,
    scope_specs: &[FieldSpec],
    scope_values: &BTreeMap<String, Value>,
    root_specs: &[FieldSpec],
    root_values: &BTreeMap<String, Value>,
) -> bool {
    let (target_spec, cur_val) = resolve_effective_condition_value(
        &rule.field,
        scope_specs,
        scope_values,
        root_specs,
        root_values,
    );
    match rule.op {
        ConditionOp::Empty => is_json_value_empty(&cur_val),
        ConditionOp::NotEmpty => !is_json_value_empty(&cur_val),
        ConditionOp::Eq => rule
            .value
            .as_ref()
            .is_some_and(|expected| scalar_equal(&cur_val, expected)),
        ConditionOp::Neq => rule
            .value
            .as_ref()
            .is_some_and(|expected| !scalar_equal(&cur_val, expected)),
        ConditionOp::In => rule
            .value
            .as_ref()
            .and_then(Value::as_array)
            .is_some_and(|list| match &cur_val {
                Value::Array(arr) => arr
                    .iter()
                    .any(|item| list.iter().any(|cand| scalar_equal(item, cand))),
                _ => list.iter().any(|cand| scalar_equal(&cur_val, cand)),
            }),
        ConditionOp::Contains => {
            let Some(needle) = rule.value.as_ref() else {
                return false;
            };
            match (&cur_val, needle) {
                (Value::String(hay), Value::String(n)) => hay.contains(n.as_str()),
                (Value::Array(items), _) => items.iter().any(|item| scalar_equal(item, needle)),
                _ => false,
            }
        }
        ConditionOp::Gt | ConditionOp::Lt => {
            let Some(expected) = rule.value.as_ref() else {
                return false;
            };
            let is_gt = rule.op == ConditionOp::Gt;
            let ord = match target_spec.map(|s| s.field_type) {
                Some(FieldType::Datetime) => {
                    match (
                        cur_val.as_str().and_then(parse_utc_datetime_millis),
                        expected.as_str().and_then(parse_utc_datetime_millis),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(&b)),
                        _ => None,
                    }
                }
                Some(FieldType::Time) => {
                    match (
                        cur_val.as_str().and_then(parse_time_seconds),
                        expected.as_str().and_then(parse_time_seconds),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(&b)),
                        _ => None,
                    }
                }
                Some(FieldType::Date) => {
                    match (
                        cur_val.as_str().filter(|s| valid_iso_date(s)),
                        expected.as_str().filter(|s| valid_iso_date(s)),
                    ) {
                        (Some(a), Some(b)) => Some(a.cmp(b)),
                        _ => None,
                    }
                }
                Some(FieldType::Number | FieldType::Range) | None => {
                    match (
                        cur_val.as_f64().filter(|f| f.is_finite()),
                        expected.as_f64().filter(|f| f.is_finite()),
                    ) {
                        (Some(a), Some(b)) => a.partial_cmp(&b),
                        _ => None,
                    }
                }
                _ => None,
            };
            ord.is_some_and(|o| {
                if is_gt {
                    o == std::cmp::Ordering::Greater
                } else {
                    o == std::cmp::Ordering::Less
                }
            })
        }
    }
}

pub fn value_equals_previous(current: Option<&Value>, previous: Option<&Value>) -> bool {
    match (current, previous) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let mut ca = a.clone();
            let mut cb = b.clone();
            canonicalize_value(&mut ca);
            canonicalize_value(&mut cb);
            ca == cb
        }
        _ => false,
    }
}
