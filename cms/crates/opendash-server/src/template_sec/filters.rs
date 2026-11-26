use super::*;

pub(super) fn filter_default(
    state: &minijinja::State,
    value: &Value,
    args: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    if value.is_none() {
        if args.len() > 2 {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::TooManyArguments,
                "too many arguments to default filter",
            ));
        }
        Ok(args.first().cloned().unwrap_or_else(|| Value::from("")))
    } else {
        minijinja::filters::default(state, value, args)
    }
}

pub(super) fn filter_invalid_op(msg: &'static str) -> minijinja::Error {
    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, msg)
}

pub(super) fn filter_date_format(
    value: Value,
    fmt: Option<&str>,
    tz: Option<&str>,
    default_tz: &jiff::tz::TimeZone,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if value.kind() != minijinja::value::ValueKind::String {
        return Err(filter_invalid_op("date_format requires a string value"));
    }
    let raw = value
        .as_str()
        .ok_or_else(|| filter_invalid_op("date_format requires a string value"))?;
    if raw.len() > 128 {
        return Err(filter_invalid_op("date_format input exceeds 128 bytes"));
    }
    let s = raw.trim();
    if s.is_empty() {
        return Ok(Value::from(""));
    }
    let fmt = match fmt {
        Some("") => return Err(filter_invalid_op("date_format format cannot be empty")),
        Some(f) if f.len() > 64 => {
            return Err(filter_invalid_op("date_format format exceeds 64 bytes"));
        }
        Some(f) => f,
        None => "%d/%m/%Y",
    };
    let target_tz = match tz {
        Some(t) => {
            if t.len() > 64 {
                return Err(filter_invalid_op("date_format timezone exceeds 64 bytes"));
            }
            let trimmed_tz = t.trim();
            if trimmed_tz.is_empty() {
                default_tz.clone()
            } else {
                jiff::tz::TimeZone::get(trimmed_tz)
                    .map_err(|_| filter_invalid_op("date_format invalid timezone"))?
            }
        }
        None => default_tz.clone(),
    };

    if s.len() == 10
        && s.is_ascii()
        && let Ok(date) = jiff::civil::Date::strptime("%Y-%m-%d", s)
    {
        let dt = date.at(0, 0, 0, 0);
        let formatted = jiff::fmt::strtime::format(fmt, dt)
            .map_err(|_| filter_invalid_op("date_format invalid format for civil date"))?;
        return Ok(Value::from(formatted));
    }
    if let Ok(ts) = s.parse::<jiff::Timestamp>() {
        let zdt = ts.to_zoned_datetime(target_tz);
        let formatted = jiff::fmt::strtime::format(fmt, &zdt)
            .map_err(|_| filter_invalid_op("date_format failed to format timestamp"))?;
        return Ok(Value::from(formatted));
    }
    if let Ok(naive) = s.parse::<jiff::civil::DateTime>() {
        let utc_zdt = naive
            .to_zoned_datetime(jiff::tz::TimeZone::UTC)
            .map_err(|_| filter_invalid_op("date_format failed to normalize naive datetime"))?;
        let zdt = utc_zdt.with_time_zone(target_tz);
        let formatted = jiff::fmt::strtime::format(fmt, &zdt)
            .map_err(|_| filter_invalid_op("date_format failed to format datetime"))?;
        return Ok(Value::from(formatted));
    }
    Ok(Value::from(""))
}

pub(super) fn filter_truncate(
    value: Value,
    max_chars: Option<i64>,
    suffix: Option<&str>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if value.kind() != minijinja::value::ValueKind::String {
        return Err(filter_invalid_op("truncate requires a string value"));
    }
    let s = value
        .as_str()
        .ok_or_else(|| filter_invalid_op("truncate requires a string value"))?;
    if s.len() > MAX_FILTER_STRING_BYTES {
        return Err(filter_invalid_op("truncate input exceeds 128 KiB"));
    }
    let limit = match max_chars {
        Some(n) if n <= 0 => return Err(filter_invalid_op("truncate length must be positive")),
        Some(n) => (n as usize).min(10_000),
        None => 160,
    };
    let suffix = suffix.unwrap_or("...");
    if suffix.chars().count() > 32 {
        return Err(filter_invalid_op("truncate suffix exceeds 32 chars"));
    }
    let half = limit / 2;
    let mut hard_end = None;
    let mut last_ws_after_half = None;
    for (count, (byte_idx, ch)) in s.char_indices().enumerate() {
        if count == limit {
            hard_end = Some(byte_idx);
            break;
        }
        if ch.is_whitespace() && count >= half {
            last_ws_after_half = Some(byte_idx);
        }
    }
    let Some(hard_end) = hard_end else {
        return Ok(Value::from(s));
    };
    let cut = last_ws_after_half.unwrap_or(hard_end);
    let prefix = s[..cut].trim_end();
    let mut out = String::with_capacity(prefix.len() + suffix.len());
    out.push_str(prefix);
    out.push_str(suffix);
    Ok(Value::from(out))
}

pub(super) fn group_thousands_and_join(
    negative: bool,
    int_part: &str,
    frac_part: &str,
    thousands_sep: &str,
    decimal_sep: &str,
) -> String {
    let mut out = String::with_capacity(
        usize::from(negative)
            + int_part.len()
            + (int_part.len() / 3) * thousands_sep.len()
            + decimal_sep.len()
            + frac_part.len(),
    );
    if negative {
        out.push('-');
    }
    for (idx, chunk) in int_part.as_bytes().rchunks(3).rev().enumerate() {
        if idx > 0 {
            out.push_str(thousands_sep);
        }
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
    }
    if !frac_part.is_empty() {
        out.push_str(decimal_sep);
        out.push_str(frac_part);
    }
    out
}

pub(super) fn filter_format_number(
    value: Value,
    decimals: Option<i64>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
) -> Result<Value, minijinja::Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(Value::from(""));
    }
    if matches!(
        value.kind(),
        minijinja::value::ValueKind::Bool
            | minijinja::value::ValueKind::Seq
            | minijinja::value::ValueKind::Map
            | minijinja::value::ValueKind::Bytes
    ) {
        return Err(filter_invalid_op("format_number requires number or string"));
    }
    let dec = decimals.unwrap_or(0);
    if !(0..=6).contains(&dec) {
        return Err(filter_invalid_op("format_number decimals must be 0..=6"));
    }
    let thousands_sep = thousands_sep.unwrap_or(".");
    let decimal_sep = decimal_sep.unwrap_or(",");
    if thousands_sep.chars().count() > 4 || decimal_sep.chars().count() > 4 {
        return Err(filter_invalid_op("format_number separator exceeds 4 chars"));
    }
    let dec_usize = dec as usize;

    enum ParsedNum {
        Int(i128),
        Float(f64),
        NonNumeric,
    }

    let parsed = match value.kind() {
        minijinja::value::ValueKind::Number => {
            if value.is_integer() {
                if let Ok(i) = i128::try_from(value.clone()) {
                    ParsedNum::Int(i)
                } else {
                    ParsedNum::Float(f64::try_from(value)?)
                }
            } else {
                ParsedNum::Float(f64::try_from(value)?)
            }
        }
        minijinja::value::ValueKind::String => {
            let s = value.as_str().unwrap_or("");
            if s.len() > 64 {
                return Err(filter_invalid_op("format_number string exceeds 64 bytes"));
            }
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(Value::from(""));
            }
            if !trimmed.contains('.') && !trimmed.contains(['e', 'E']) {
                if let Ok(i) = trimmed.parse::<i128>() {
                    ParsedNum::Int(i)
                } else if let Ok(f) = trimmed.parse::<f64>() {
                    ParsedNum::Float(f)
                } else {
                    ParsedNum::NonNumeric
                }
            } else if let Ok(f) = trimmed.parse::<f64>() {
                ParsedNum::Float(f)
            } else {
                ParsedNum::NonNumeric
            }
        }
        _ => return Err(filter_invalid_op("format_number unsupported value type")),
    };

    match parsed {
        ParsedNum::NonNumeric => Ok(Value::from("")),
        ParsedNum::Int(i) => {
            let abs_digits = i.unsigned_abs().to_string();
            if abs_digits.len() > 32 {
                return Err(filter_invalid_op("format_number integer exceeds 32 digits"));
            }
            let frac = if dec_usize > 0 {
                "0".repeat(dec_usize)
            } else {
                String::new()
            };
            let negative = i < 0;
            Ok(Value::from(group_thousands_and_join(
                negative,
                &abs_digits,
                &frac,
                thousands_sep,
                decimal_sep,
            )))
        }
        ParsedNum::Float(num) => {
            if !num.is_finite() {
                return Err(filter_invalid_op("format_number value must be finite"));
            }
            let formatted = format!("{:.*}", dec_usize, num.abs());
            let (int_part, frac_part) = formatted.split_once('.').unwrap_or((&formatted, ""));
            if int_part.len() > 32 {
                return Err(filter_invalid_op(
                    "format_number integer part exceeds 32 digits",
                ));
            }
            let negative = num.is_sign_negative()
                && (int_part.bytes().any(|b| b != b'0') || frac_part.bytes().any(|b| b != b'0'));
            Ok(Value::from(group_thousands_and_join(
                negative,
                int_part,
                frac_part,
                thousands_sep,
                decimal_sep,
            )))
        }
    }
}

pub(super) fn read_bounded_sequence(
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

pub(super) fn filter_batch(
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

pub(super) fn parse_validated_attr_path(attr: &str) -> Result<Vec<&str>, minijinja::Error> {
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

pub(super) fn resolve_attr_path(item: &Value, segments: &[&str]) -> Value {
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

pub(super) fn eval_attr_test_predicate(
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

pub(super) fn filter_sequence_by_attr(
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

pub(super) fn filter_selectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, false)
}

pub(super) fn filter_rejectattr(
    state: &minijinja::State,
    value: Value,
    attr: &str,
    extra: minijinja::value::Rest<Value>,
) -> Result<Value, minijinja::Error> {
    filter_sequence_by_attr(state, value, attr, extra, true)
}

pub(super) fn filter_map(
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

pub(super) fn collect_composition_sites(
    stmt: &Stmt<'_>,
    source: &str,
    in_for_or_set_block: bool,
    sites: &mut Vec<CompositionSite>,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::EmitExpr(e) => {
            if parse_od_icon_call_ext(&e.expr, true)?.is_some()
                || parse_od_hook_call(&e.expr)?.is_some()
            {
                return Ok(());
            }
            let Some(call) = parse_composition_call(&e.expr)? else {
                return Ok(());
            };
            if in_for_or_set_block {
                return Err("od_part/od_content không được nằm trong for hoặc set block.");
            }
            let start = e.span().start_offset as usize;
            let expr_end = e.span().end_offset as usize;
            if start >= source.len()
                || expr_end > source.len()
                || !source[start..].starts_with("{{")
            {
                return Err("od_part/od_content chỉ được xuất hiện ở phần thân HTML.");
            }
            // The AST span covers '{{' through the final expression token,
            // but not the closing '}}'. Search only after that token so a
            // quoted string containing '}}' cannot truncate the replacement.
            let suffix = source
                .get(expr_end..)
                .ok_or("Vị trí biểu thức composition không hợp lệ.")?;
            let close = suffix
                .find("}}")
                .ok_or("od_part/od_content thiếu dấu đóng.")?;
            if !suffix[..close]
                .chars()
                .all(|ch| ch.is_whitespace() || ch == '-')
            {
                return Err("Biểu thức composition chứa token ngoài allowlist.");
            }
            sites.push(CompositionSite {
                start,
                end: expr_end + close + 2,
                call,
            });
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(expr) = &f.filter_expr {
                validate_expr(expr)?;
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::WithBlock(w) => {
            for (key, val) in &w.assignments {
                validate_expr(key)?;
                validate_expr(val)?;
            }
            for child in &w.body {
                collect_composition_sites(child, source, in_for_or_set_block, sites)?;
            }
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                collect_composition_sites(child, source, true, sites)?;
            }
        }
        _ => validate_stmt(stmt)?,
    }
    Ok(())
}

pub(super) fn validate_stmt_with_icons_ext(
    stmt: &Stmt<'_>,
    allow_od_icon: bool,
    allow_dynamic_icon: bool,
    in_set_block: bool,
) -> Result<(), &'static str> {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::EmitRaw(_) => Ok(()),
        Stmt::EmitExpr(e) => {
            if allow_od_icon
                && !in_set_block
                && (parse_od_icon_call_ext(&e.expr, allow_dynamic_icon)?.is_some()
                    || parse_od_hook_call(&e.expr)?.is_some())
            {
                Ok(())
            } else {
                validate_expr(&e.expr)
            }
        }
        Stmt::ForLoop(f) => {
            validate_expr(&f.target)?;
            validate_expr(&f.iter)?;
            if let Some(cond) = &f.filter_expr {
                validate_expr(cond)?;
            }
            for child in &f.body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            for child in &f.else_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::IfCond(i) => {
            validate_expr(&i.expr)?;
            for child in &i.true_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            for child in &i.false_body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::WithBlock(w) => {
            for (k, v) in &w.assignments {
                validate_expr(k)?;
                validate_expr(v)?;
            }
            for child in &w.body {
                validate_stmt_with_icons_ext(
                    child,
                    allow_od_icon,
                    allow_dynamic_icon,
                    in_set_block,
                )?;
            }
            Ok(())
        }
        Stmt::Set(s) => {
            validate_expr(&s.target)?;
            validate_expr(&s.expr)
        }
        Stmt::SetBlock(sb) => {
            if sb.filter.is_some() {
                return Err("C-SEC cấm modifier filter trong khối set.");
            }
            validate_expr(&sb.target)?;
            for child in &sb.body {
                validate_stmt_with_icons_ext(child, allow_od_icon, allow_dynamic_icon, true)?;
            }
            Ok(())
        }
        Stmt::AutoEscape(_)
        | Stmt::FilterBlock(_)
        | Stmt::Macro(_)
        | Stmt::CallBlock(_)
        | Stmt::Import(_)
        | Stmt::FromImport(_)
        | Stmt::Extends(_)
        | Stmt::Include(_)
        | Stmt::Block(_)
        | Stmt::Do(_) => Err("Template sử dụng câu lệnh MiniJinja bị C-SEC cấm."),
    }
}

pub(super) fn collect_t_keys_stmt(stmt: &Stmt<'_>, out: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Template(t) => {
            for child in &t.children {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::EmitExpr(e) => collect_t_keys_expr(&e.expr, out),
        Stmt::ForLoop(f) => {
            collect_t_keys_expr(&f.iter, out);
            if let Some(cond) = &f.filter_expr {
                collect_t_keys_expr(cond, out);
            }
            for child in f.body.iter().chain(&f.else_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::IfCond(i) => {
            collect_t_keys_expr(&i.expr, out);
            for child in i.true_body.iter().chain(&i.false_body) {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::WithBlock(w) => {
            for (_, val) in &w.assignments {
                collect_t_keys_expr(val, out);
            }
            for child in &w.body {
                collect_t_keys_stmt(child, out);
            }
        }
        Stmt::Set(s) => collect_t_keys_expr(&s.expr, out),
        Stmt::SetBlock(sb) => {
            if let Some(filter) = &sb.filter {
                collect_t_keys_expr(filter, out);
            }
            for child in &sb.body {
                collect_t_keys_stmt(child, out);
            }
        }
        _ => {}
    }
}
