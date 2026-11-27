use super::*;

pub(crate) fn read_bounded_sequence(
    value: &Value,
    name: &str,
) -> Result<Vec<Value>, minijinja::Error> {
    if value.kind() != minijinja::value::ValueKind::Seq {
        return Err(filter_invalid_op(if name == "batch" {
            "batch requires a sequence"
        } else {
            "sequence filter requires a sequence"
        }));
    }
    let iter = value.try_iter()?;
    let mut items = Vec::new();
    for item in iter.take(MAX_FILTER_SEQ_ITEMS + 1) {
        if items.len() == MAX_FILTER_SEQ_ITEMS {
            return Err(filter_invalid_op("sequence exceeds 500 items limit"));
        }
        items.push(item);
    }
    Ok(items)
}

pub(crate) fn filter_batch(
    value: Value,
    size: i64,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    if extra.len() > 1 {
        return Err(filter_invalid_op(
            "batch accepts at most 1 extra fill argument",
        ));
    }
    if size <= 0 || size > 100 {
        return Err(filter_invalid_op("batch size must be in 1..=100"));
    }
    let items = read_bounded_sequence(&value, "batch")?;
    let fill = if extra.len() == 1 && !extra[0].is_undefined() {
        Some(extra[0].clone())
    } else {
        None
    };
    let chunk_size = size as usize;
    let mut chunks = Vec::with_capacity(items.len().div_ceil(chunk_size));
    for slice in items.chunks(chunk_size) {
        let mut chunk = slice.to_vec();
        if chunk.len() < chunk_size
            && let Some(pad) = &fill
        {
            while chunk.len() < chunk_size {
                chunk.push(pad.clone());
            }
        }
        chunks.push(Value::from(chunk));
    }
    Ok(Value::from(chunks))
}

pub(crate) fn parse_validated_attr_path(attr: &str) -> Result<Vec<&str>, minijinja::Error> {
    let segments: Vec<&str> = attr.split('.').collect();
    if !(1..=3).contains(&segments.len()) {
        return Err(filter_invalid_op("attribute path must have 1..=3 segments"));
    }
    for seg in &segments {
        if seg.is_empty()
            || seg.len() > 64
            || seg.starts_with("__")
            || !seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(filter_invalid_op("invalid attribute path segment"));
        }
    }
    Ok(segments)
}

pub(crate) fn resolve_attr_path(item: &Value, segments: &[&str]) -> Value {
    let mut cur = item.clone();
    for seg in segments {
        if cur.kind() != minijinja::value::ValueKind::Map {
            return Value::UNDEFINED;
        }
        match cur.get_attr(seg) {
            Ok(next) => cur = next,
            Err(_) => return Value::UNDEFINED,
        }
    }
    cur
}

pub(crate) fn eval_attr_test_predicate(
    state: &minijinja::State,
    attr_val: &Value,
    test_name: Option<&str>,
    rhs: Option<&Value>,
) -> Result<bool, minijinja::Error> {
    let Some(test_name) = test_name else {
        return Ok(attr_val.is_true());
    };
    match rhs {
        None => Ok(match test_name {
            "defined" => minijinja::tests::is_defined(attr_val),
            "undefined" => minijinja::tests::is_undefined(attr_val),
            "even" => minijinja::tests::is_even(attr_val.clone()),
            "odd" => minijinja::tests::is_odd(attr_val.clone()),
            "number" => minijinja::tests::is_number(attr_val),
            "string" => minijinja::tests::is_string(attr_val),
            "sequence" => minijinja::tests::is_sequence(attr_val),
            "mapping" => minijinja::tests::is_mapping(attr_val),
            "none" => minijinja::tests::is_none(attr_val),
            "true" => minijinja::tests::is_true(attr_val),
            "false" => minijinja::tests::is_false(attr_val),
            _ => return Err(filter_invalid_op("disallowed unary test")),
        }),
        Some(rhs) => match test_name {
            "eq" | "equalto" => Ok(minijinja::tests::is_eq(attr_val, rhs)),
            "ne" => Ok(minijinja::tests::is_ne(attr_val, rhs)),
            "lt" => Ok(minijinja::tests::is_lt(attr_val, rhs)),
            "le" => Ok(minijinja::tests::is_le(attr_val, rhs)),
            "gt" => Ok(minijinja::tests::is_gt(attr_val, rhs)),
            "ge" => Ok(minijinja::tests::is_ge(attr_val, rhs)),
            "in" => minijinja::tests::is_in(state, attr_val, rhs),
            "startingwith" => {
                let prefix = rhs
                    .as_str()
                    .ok_or_else(|| filter_invalid_op("startingwith requires string operand"))?;
                Ok(attr_val.as_str().is_some_and(|s| {
                    minijinja::tests::is_startingwith(
                        std::borrow::Cow::Borrowed(s),
                        std::borrow::Cow::Borrowed(prefix),
                    )
                }))
            }
            "endingwith" => {
                let suffix = rhs
                    .as_str()
                    .ok_or_else(|| filter_invalid_op("endingwith requires string operand"))?;
                Ok(attr_val.as_str().is_some_and(|s| {
                    minijinja::tests::is_endingwith(
                        std::borrow::Cow::Borrowed(s),
                        std::borrow::Cow::Borrowed(suffix),
                    )
                }))
            }
            _ => Err(filter_invalid_op("disallowed binary test")),
        },
    }
}

pub(crate) fn filter_sequence_by_attr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
    invert: bool,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    let segments = parse_validated_attr_path(attr)?;
    let (test_name, rhs): (Option<&str>, Option<&Value>) = match extra.len() {
        0 => (None, None),
        1 => {
            let name = extra[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("test name must be a string"))?;
            if !UNARY_ALLOWED_TESTS.contains(&name) {
                return Err(filter_invalid_op("test is not an allowed unary C-SEC test"));
            }
            (Some(name), None)
        }
        2 => {
            let name = extra[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("test name must be a string"))?;
            if !BINARY_ALLOWED_TESTS.contains(&name) {
                return Err(filter_invalid_op(
                    "test is not an allowed binary C-SEC test",
                ));
            }
            if matches!(name, "startingwith" | "endingwith") && extra[1].as_str().is_none() {
                return Err(filter_invalid_op("string test requires string operand"));
            }
            (Some(name), Some(&extra[1]))
        }
        _ => return Err(filter_invalid_op("too many arguments to attr filter")),
    };
    let items = read_bounded_sequence(&value, "attr")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let attr_val = resolve_attr_path(&item, &segments);
        let matched = eval_attr_test_predicate(state, &attr_val, test_name, rhs)?;
        if matched ^ invert {
            out.push(item);
        }
    }
    Ok(Value::from(out))
}

pub(crate) fn filter_selectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, false)
}

pub(crate) fn filter_rejectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, true)
}

pub(crate) fn filter_map(
    value: Value,
    rest: minijinja::value::Rest<minijinja::value::ValueOrKwargs>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(Vec::<Value>::new()));
    }
    let raw = rest.into_values();
    let (args, kwargs) = minijinja::value::from_args::<(&[Value], minijinja::value::Kwargs)>(&raw)?;
    let (attr, default_val) = match args.len() {
        1 => {
            let attr = args[0]
                .as_str()
                .ok_or_else(|| filter_invalid_op("map positional argument must be a string"))?;
            kwargs.assert_all_used()?;
            (attr, Value::from(()))
        }
        0 => {
            let attr: &str = kwargs.get("attribute")?;
            let default_opt: Option<Value> = kwargs.get("default")?;
            kwargs.assert_all_used()?;
            (attr, default_opt.unwrap_or_else(|| Value::from(())))
        }
        _ => return Err(filter_invalid_op("map requires attribute argument")),
    };
    let segments = parse_validated_attr_path(attr)?;
    let items = read_bounded_sequence(&value, "map")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let resolved = resolve_attr_path(&item, &segments);
        if resolved.is_undefined() {
            out.push(default_val.clone());
        } else {
            out.push(resolved);
        }
    }
    Ok(Value::from(out))
}
