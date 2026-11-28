//! Pure `jiff`-backed date, datetime, time, and locale number formatters for S02–S03 blocks.

use std::str::FromStr;

use opendash_core::field_schema::FieldType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateFormatPreset {
    #[default]
    DmySlash,
    MdyLong,
    IsoDate,
    ShortMonth,
}

impl DateFormatPreset {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.unwrap_or("dmy_slash").trim() {
            "mdy_long" => Self::MdyLong,
            "iso_date" => Self::IsoDate,
            "short_month" => Self::ShortMonth,
            _ => Self::DmySlash,
        }
    }
}

fn is_vi_locale(locale: &str) -> bool {
    let lower = locale.trim().to_ascii_lowercase();
    lower.is_empty() || lower == "vi" || lower.starts_with("vi-")
}

fn vi_month_full(month: i8) -> &'static str {
    match month {
        1 => "tháng 1",
        2 => "tháng 2",
        3 => "tháng 3",
        4 => "tháng 4",
        5 => "tháng 5",
        6 => "tháng 6",
        7 => "tháng 7",
        8 => "tháng 8",
        9 => "tháng 9",
        10 => "tháng 10",
        11 => "tháng 11",
        12 => "tháng 12",
        _ => "",
    }
}

fn vi_month_short(month: i8) -> &'static str {
    match month {
        1 => "Thg 1",
        2 => "Thg 2",
        3 => "Thg 3",
        4 => "Thg 4",
        5 => "Thg 5",
        6 => "Thg 6",
        7 => "Thg 7",
        8 => "Thg 8",
        9 => "Thg 9",
        10 => "Thg 10",
        11 => "Thg 11",
        12 => "Thg 12",
        _ => "",
    }
}

fn en_month_full(month: i8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn en_month_short(month: i8) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "",
    }
}

fn parse_civil_date_in_tz(
    raw: &str,
    tz_name: &str,
) -> Option<(jiff::civil::Date, Option<jiff::civil::Time>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let tz = jiff::tz::TimeZone::get(tz_name).unwrap_or(jiff::tz::TimeZone::UTC);
    if let Ok(ts) = jiff::Timestamp::from_str(trimmed) {
        let zdt = ts.to_zoned_datetime(tz);
        return Some((zdt.date(), Some(zdt.time())));
    }
    if let Ok(dt) = jiff::civil::DateTime::from_str(trimmed) {
        let zdt = dt.to_zoned_datetime(tz).ok()?;
        return Some((zdt.date(), Some(zdt.time())));
    }
    // SQLite `YYYY-MM-DD HH:MM:SS` is stored in UTC when lacking an offset.
    if let Some((d_part, t_part)) = trimmed.split_once(' ')
        && let Ok(date) = jiff::civil::Date::from_str(d_part)
        && let Ok(time) = jiff::civil::Time::from_str(t_part)
    {
        let utc_dt = date.to_datetime(time);
        if let Ok(utc_zdt) = utc_dt.to_zoned_datetime(jiff::tz::TimeZone::UTC) {
            let local_zdt = utc_zdt.with_time_zone(tz);
            return Some((local_zdt.date(), Some(local_zdt.time())));
        }
        return Some((date, Some(time)));
    }
    if let Ok(date) = jiff::civil::Date::from_str(trimmed) {
        return Some((date, None));
    }
    None
}

pub fn format_date_with_preset(
    raw: &str,
    preset: DateFormatPreset,
    tz_name: &str,
    locale: &str,
) -> Option<String> {
    let (date, _) = parse_civil_date_in_tz(raw, tz_name)?;
    let year = date.year();
    let month = date.month();
    let day = date.day();
    let is_vi = is_vi_locale(locale);
    Some(match preset {
        DateFormatPreset::DmySlash => format!("{day:02}/{month:02}/{year:04}"),
        DateFormatPreset::IsoDate => format!("{year:04}-{month:02}-{day:02}"),
        DateFormatPreset::MdyLong => {
            if is_vi {
                format!("{day} {}, {year:04}", vi_month_full(month))
            } else {
                format!("{} {day}, {year:04}", en_month_full(month))
            }
        }
        DateFormatPreset::ShortMonth => {
            if is_vi {
                format!("{day} {}, {year:04}", vi_month_short(month))
            } else {
                format!("{} {day}, {year:04}", en_month_short(month))
            }
        }
    })
}

pub fn format_temporal_field(
    field_type: FieldType,
    raw: &str,
    preset: DateFormatPreset,
    tz_name: &str,
    locale: &str,
) -> String {
    match field_type {
        FieldType::Time => {
            let trimmed = raw.trim();
            if let Ok(t) = jiff::civil::Time::from_str(trimmed) {
                return format!("{:02}:{:02}", t.hour(), t.minute());
            }
            trimmed.to_owned()
        }
        FieldType::Datetime => {
            if let Some((date, time_opt)) = parse_civil_date_in_tz(raw, tz_name) {
                let d_str = format_date_with_preset(&date.to_string(), preset, tz_name, locale)
                    .unwrap_or_else(|| date.to_string());
                if let Some(t) = time_opt {
                    return format!("{d_str} {:02}:{:02}", t.hour(), t.minute());
                }
                return d_str;
            }
            raw.trim().to_owned()
        }
        _ => format_date_with_preset(raw, preset, tz_name, locale)
            .unwrap_or_else(|| raw.trim().to_owned()),
    }
}

pub fn format_number_locale(value: f64, locale: &str) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let normalized = if value == 0.0 { 0.0 } else { value };
    let raw = format!("{normalized}");
    let (neg, unsigned) = if let Some(rest) = raw.strip_prefix('-') {
        (true, rest)
    } else {
        (false, raw.as_str())
    };
    let (int_part, frac_part) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let is_vi = is_vi_locale(locale);
    let (thousands_sep, dec_sep) = if is_vi { ('.', ',') } else { (',', '.') };

    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (idx, ch) in int_part.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(thousands_sep);
        }
        grouped.push(ch);
    }
    let int_fmt: String = grouped.chars().rev().collect();
    let mut out = if neg { format!("-{int_fmt}") } else { int_fmt };
    if !frac_part.is_empty() {
        out.push(dec_sep);
        out.push_str(frac_part);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates_across_four_presets_vi_and_en() {
        let iso = "2026-11-26T15:30:00Z";
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::DmySlash, "UTC", "vi").as_deref(),
            Some("26/11/2026")
        );
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::IsoDate, "UTC", "en").as_deref(),
            Some("2026-11-26")
        );
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::MdyLong, "UTC", "vi").as_deref(),
            Some("26 tháng 11, 2026")
        );
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::MdyLong, "UTC", "en").as_deref(),
            Some("November 26, 2026")
        );
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::ShortMonth, "UTC", "vi").as_deref(),
            Some("26 Thg 11, 2026")
        );
        assert_eq!(
            format_date_with_preset(iso, DateFormatPreset::ShortMonth, "UTC", "en").as_deref(),
            Some("Nov 26, 2026")
        );
        assert_eq!(format_number_locale(1234567.5, "vi"), "1.234.567,5");
        assert_eq!(format_number_locale(1234567.5, "en"), "1,234,567.5");
    }
}
