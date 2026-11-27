use super::*;

pub(crate) fn filter_default(
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

pub(crate) fn filter_invalid_op(msg: &'static str) -> minijinja::Error {
    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, msg)
}

pub(crate) fn filter_date_format(
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

pub(crate) fn filter_truncate(
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

pub(crate) fn group_thousands_and_join(
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

pub(crate) fn filter_format_number(
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
