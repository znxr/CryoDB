use crate::{
    ColumnKind, DatabaseDriver, binary_preview, binary_text_value, is_binary_preview_value,
    is_null_value, is_postgres_vector_type_name, sql_quote_string_literal,
};
use sqlx::mysql::MySqlRow;
use sqlx::postgres::types::{Oid, PgInterval, PgMoney, PgTimeTz};
use sqlx::postgres::{PgRow, Postgres};
use sqlx::sqlite::SqliteRow;
use sqlx::types::BigDecimal;
use sqlx::types::chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::{Decode, Row, Type, TypeInfo, ValueRef};
use std::fmt::Write;

pub(crate) fn row_to_strings(
    row: &MySqlRow,
    column_kinds: &[ColumnKind],
    column_count: usize,
) -> Vec<String> {
    let mut values = Vec::with_capacity(column_count);
    for index in 0..column_count {
        let kind = column_kinds
            .get(index)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        values.push(value_to_string(row, index, kind));
    }
    values
}

fn value_to_string(row: &MySqlRow, index: usize, kind: ColumnKind) -> String {
    match kind {
        ColumnKind::Text => decode_text_value(row, index),
        ColumnKind::Binary => decode_binary_value(row, index),
        ColumnKind::Integer => decode_integer_value(row, index),
        ColumnKind::Unsigned => decode_unsigned_value(row, index),
        ColumnKind::Float => decode_float_value(row, index),
        ColumnKind::Decimal => decode_decimal_value(row, index),
        ColumnKind::Bool => decode_bool_value(row, index),
        ColumnKind::DateTime => decode_datetime_value(row, index),
        ColumnKind::Date => decode_date_value(row, index),
        ColumnKind::Time => decode_time_value(row, index),
        ColumnKind::Unknown => value_to_string_fallback(row, index),
    }
}

fn decode_text_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<&str>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => match row.try_get_unchecked::<Option<&[u8]>, _>(index) {
            Ok(Some(bytes)) => String::from_utf8_lossy(bytes).to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
    }
}

fn decode_binary_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<&[u8]>, _>(index) {
        Ok(Some(bytes)) => binary_text_value(bytes).unwrap_or_else(|| binary_preview(bytes)),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_integer_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<i64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_unsigned_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<u64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_float_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<f64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_decimal_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<BigDecimal>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_bool_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<bool>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_datetime_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<DateTime<Utc>>, _>(index) {
        Ok(Some(value)) => value.format("%Y-%m-%d %H:%M:%S").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_date_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<NaiveDate>, _>(index) {
        Ok(Some(value)) => value.format("%Y-%m-%d").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn decode_time_value(row: &MySqlRow, index: usize) -> String {
    match row.try_get_unchecked::<Option<NaiveTime>, _>(index) {
        Ok(Some(value)) => value.format("%H:%M:%S").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback(row, index),
    }
}

fn value_to_string_fallback(row: &MySqlRow, index: usize) -> String {
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<u64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<BigDecimal>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<bool>, _>(index) {
        return value
            .map(|flag| flag.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveDateTime>, _>(index) {
        return value
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveDate>, _>(index) {
        return value
            .map(|date| date.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveTime>, _>(index) {
        return value
            .map(|time| time.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<DateTime<Utc>>, _>(index) {
        return value
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }

    String::from("<unprintable>")
}

pub(crate) fn row_to_strings_sqlite(
    row: &SqliteRow,
    column_kinds: &[ColumnKind],
    column_count: usize,
) -> Vec<String> {
    let mut values = Vec::with_capacity(column_count);
    for index in 0..column_count {
        let kind = column_kinds
            .get(index)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        values.push(value_to_string_sqlite(row, index, kind));
    }
    values
}

fn value_to_string_sqlite(row: &SqliteRow, index: usize, kind: ColumnKind) -> String {
    match kind {
        ColumnKind::Binary => match row.try_get::<Option<Vec<u8>>, _>(index) {
            Ok(Some(bytes)) => binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes)),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_sqlite(row, index),
        },
        ColumnKind::Integer | ColumnKind::Unsigned => match row.try_get::<Option<i64>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_sqlite(row, index),
        },
        ColumnKind::Float | ColumnKind::Decimal => match row.try_get::<Option<f64>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_sqlite(row, index),
        },
        ColumnKind::Bool => match row.try_get::<Option<bool>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => match row.try_get::<Option<i64>, _>(index) {
                Ok(Some(value)) => (value != 0).to_string(),
                Ok(None) => String::from("NULL"),
                Err(_) => value_to_string_fallback_sqlite(row, index),
            },
        },
        _ => match row.try_get::<Option<String>, _>(index) {
            Ok(Some(value)) => value,
            Ok(None) => String::from("NULL"),
            Err(_) => match row.try_get::<Option<Vec<u8>>, _>(index) {
                Ok(Some(bytes)) => {
                    binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes))
                }
                Ok(None) => String::from("NULL"),
                Err(_) => value_to_string_fallback_sqlite(row, index),
            },
        },
    }
}

fn value_to_string_fallback_sqlite(row: &SqliteRow, index: usize) -> String {
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<bool>, _>(index) {
        return value
            .map(|flag| flag.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value
            .map(|bytes| binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes)))
            .unwrap_or_else(|| String::from("NULL"));
    }

    String::from("<unprintable>")
}

pub(crate) fn row_to_strings_postgres(
    row: &PgRow,
    column_kinds: &[ColumnKind],
    column_type_names: &[String],
    column_count: usize,
) -> Vec<String> {
    let mut values = Vec::with_capacity(column_count);
    for index in 0..column_count {
        let kind = column_kinds
            .get(index)
            .copied()
            .unwrap_or(ColumnKind::Unknown);
        let type_name = column_type_names
            .get(index)
            .map(String::as_str)
            .unwrap_or("");
        let mut value = value_to_string_postgres(row, index, kind);
        if !is_null_value(&value) && is_postgres_vector_type_name(type_name) {
            value = format_pgvector_display(&value);
        }
        values.push(value);
    }
    values
}

fn format_array_values<T: ToString>(values: Vec<T>) -> String {
    let values = values
        .into_iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    format!("[{}]", values.join(", "))
}

fn format_pgvector_display(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
        let normalized = inner
            .split(',')
            .map(|entry| entry.trim())
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        return format!("[{normalized}]");
    }

    trimmed.to_string()
}

fn decode_text_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<String>, _>(index) {
        Ok(Some(value)) => value,
        Ok(None) => String::from("NULL"),
        Err(_) => match row.try_get::<Option<Vec<u8>>, _>(index) {
            Ok(Some(bytes)) => binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes)),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_postgres(row, index),
        },
    }
}

fn decode_binary_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<Vec<u8>>, _>(index) {
        Ok(Some(bytes)) => binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes)),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn decode_integer_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<i64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => match row.try_get::<Option<i32>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => match row.try_get::<Option<i16>, _>(index) {
                Ok(Some(value)) => value.to_string(),
                Ok(None) => String::from("NULL"),
                Err(_) => value_to_string_fallback_postgres(row, index),
            },
        },
    }
}

fn decode_unsigned_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<i64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn decode_float_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<f64>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => match row.try_get::<Option<f32>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_postgres(row, index),
        },
    }
}

fn decode_decimal_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<BigDecimal>, _>(index) {
        Ok(Some(value)) => {
            let raw = row.try_get_raw(index).ok();
            let scale = raw.and_then(|raw| {
                Some(i16::from_be_bytes(
                    raw.as_bytes().ok()?.get(6..8)?.try_into().ok()?,
                ))
            });
            scale
                .map_or(value.clone(), |scale| value.with_scale(scale.into()))
                .to_string()
        }
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn decode_bool_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<bool>, _>(index) {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn decode_datetime_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<DateTime<Utc>>, _>(index) {
        Ok(Some(value)) => value.format("%Y-%m-%d %H:%M:%S").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => match row.try_get::<Option<NaiveDateTime>, _>(index) {
            Ok(Some(value)) => value.format("%Y-%m-%d %H:%M:%S").to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback_postgres(row, index),
        },
    }
}

fn decode_date_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<NaiveDate>, _>(index) {
        Ok(Some(value)) => value.format("%Y-%m-%d").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn decode_time_value_postgres(row: &PgRow, index: usize) -> String {
    match row.try_get::<Option<NaiveTime>, _>(index) {
        Ok(Some(value)) => value.format("%H:%M:%S").to_string(),
        Ok(None) => String::from("NULL"),
        Err(_) => value_to_string_fallback_postgres(row, index),
    }
}

fn value_to_string_postgres(row: &PgRow, index: usize, kind: ColumnKind) -> String {
    match kind {
        ColumnKind::Text => decode_text_value_postgres(row, index),
        ColumnKind::Binary => decode_binary_value_postgres(row, index),
        ColumnKind::Integer => decode_integer_value_postgres(row, index),
        ColumnKind::Unsigned => decode_unsigned_value_postgres(row, index),
        ColumnKind::Float => decode_float_value_postgres(row, index),
        ColumnKind::Decimal => decode_decimal_value_postgres(row, index),
        ColumnKind::Bool => decode_bool_value_postgres(row, index),
        ColumnKind::DateTime => decode_datetime_value_postgres(row, index),
        ColumnKind::Date => decode_date_value_postgres(row, index),
        ColumnKind::Time => decode_time_value_postgres(row, index),
        ColumnKind::Unknown => value_to_string_fallback_postgres(row, index),
    }
}

fn value_to_string_fallback_postgres(row: &PgRow, index: usize) -> String {
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<i32>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<i16>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<f64>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<f32>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<BigDecimal>, _>(index) {
        return value
            .map(|num| num.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<bool>, _>(index) {
        return value
            .map(|flag| flag.to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveDateTime>, _>(index) {
        return value
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<DateTime<Utc>>, _>(index) {
        return value
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveDate>, _>(index) {
        return value
            .map(|date| date.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<NaiveTime>, _>(index) {
        return value
            .map(|time| time.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<String>>, _>(index) {
        return value
            .map(format_array_values)
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<i64>>, _>(index) {
        return value
            .map(format_array_values)
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<f64>>, _>(index) {
        return value
            .map(format_array_values)
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<bool>>, _>(index) {
        return value
            .map(format_array_values)
            .unwrap_or_else(|| String::from("NULL"));
    }
    if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(index) {
        return value
            .map(|bytes| binary_text_value(&bytes).unwrap_or_else(|| binary_preview(&bytes)))
            .unwrap_or_else(|| String::from("NULL"));
    }

    postgres_value(row, index, format_array_values::<i32>)
        .or_else(|| postgres_value(row, index, format_array_values::<i16>))
        .or_else(|| postgres_value(row, index, format_array_values::<f32>))
        .or_else(|| postgres_value(row, index, format_array_values::<BigDecimal>))
        .or_else(|| postgres_value(row, index, |value: Oid| value.0.to_string()))
        .or_else(|| {
            postgres_value(row, index, |value: PgMoney| {
                value.to_bigdecimal(2).to_string()
            })
        })
        .or_else(|| {
            postgres_value(row, index, |value: PgTimeTz| {
                format!("{}{}", value.time, value.offset)
            })
        })
        .or_else(|| postgres_value(row, index, format_postgres_interval))
        .or_else(|| postgres_raw_value(row, index))
        .unwrap_or_else(|| String::from("<unprintable>"))
}

fn postgres_value<T>(row: &PgRow, index: usize, format: impl FnOnce(T) -> String) -> Option<String>
where
    T: for<'r> Decode<'r, Postgres> + Type<Postgres>,
{
    let value = row.try_get::<Option<T>, _>(index).ok()?;
    Some(value.map_or_else(|| String::from("NULL"), format))
}

fn format_postgres_interval(value: PgInterval) -> String {
    let unit = |amount: i64, name: &str| {
        format!(
            "{amount} {name}{}",
            if amount.abs() == 1 { "" } else { "s" }
        )
    };
    let mut parts = Vec::new();
    let (years, months) = (value.months / 12, value.months % 12);
    if years != 0 {
        parts.push(unit(years.into(), "year"));
    }
    if months != 0 {
        parts.push(unit(months.into(), "mon"));
    }
    if value.days != 0 {
        parts.push(unit(value.days.into(), "day"));
    }
    if value.microseconds != 0 || parts.is_empty() {
        let sign = if value.microseconds < 0 { "-" } else { "" };
        let micros = value.microseconds.unsigned_abs();
        let seconds = micros / 1_000_000;
        let mut time = format!(
            "{sign}{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        );
        let fraction = micros % 1_000_000;
        if fraction != 0 {
            time.push_str(format!(".{fraction:06}").trim_end_matches('0'));
        }
        parts.push(time);
    }
    parts.join(" ")
}

fn postgres_raw_value(row: &PgRow, index: usize) -> Option<String> {
    let raw = row.try_get_raw(index).ok()?;
    if raw.is_null() {
        return Some(String::from("NULL"));
    }
    let type_info = raw.type_info().into_owned();
    let bytes = raw.as_bytes().ok()?;
    match type_info.name() {
        "UUID" if bytes.len() == 16 => {
            let hex = bytes_to_hex(bytes).to_ascii_lowercase();
            Some(format!(
                "{}-{}-{}-{}-{}",
                &hex[..8],
                &hex[8..12],
                &hex[12..16],
                &hex[16..20],
                &hex[20..]
            ))
        }
        "JSONB" => binary_text_value(bytes.get(1..)?),
        "NUMERIC" => match bytes.get(4..6)? {
            [0xC0, 0] => Some(String::from("NaN")),
            [0xD0, 0] => Some(String::from("Infinity")),
            [0xF0, 0] => Some(String::from("-Infinity")),
            _ => None,
        },
        "INET" | "CIDR" => {
            let (bits, is_cidr, address) = (bytes.get(1)?, bytes.get(2)?, bytes.get(4..)?);
            let address = match address.len() {
                4 => std::net::IpAddr::from(<[u8; 4]>::try_from(address).ok()?),
                16 => std::net::IpAddr::from(<[u8; 16]>::try_from(address).ok()?),
                _ => return None,
            };
            let max_bits = if address.is_ipv4() { 32 } else { 128 };
            Some(if *is_cidr == 0 && *bits == max_bits {
                address.to_string()
            } else {
                format!("{address}/{bits}")
            })
        }
        "BIT" | "VARBIT" => {
            let length =
                usize::try_from(i32::from_be_bytes(bytes.get(..4)?.try_into().ok()?)).ok()?;
            let bits: String = bytes
                .get(4..)?
                .iter()
                .map(|byte| format!("{byte:08b}"))
                .collect();
            bits.get(..length).map(str::to_string)
        }
        "vector" => {
            let dimensions = usize::from(u16::from_be_bytes(bytes.get(..2)?.try_into().ok()?));
            let values = bytes
                .get(4..4 + dimensions * 4)?
                .chunks_exact(4)
                .map(|chunk| f32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            Some(format_array_values(values.collect()))
        }
        _ => binary_text_value(bytes),
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{:02X}", byte).ok();
    }
    output
}

pub(crate) fn coerce_sql_cell_value(value: String, _kind: ColumnKind) -> String {
    value
}

fn is_supported_sql_cell_function(name: &str) -> bool {
    matches!(
        name,
        "NOW"
            | "CURRENT_TIMESTAMP"
            | "UUID"
            | "UUID_SHORT"
            | "RAND"
            | "CURRENT_USER"
            | "DATABASE"
            | "MD5"
            | "SHA2"
            | "CONCAT"
            | "JSON_OBJECT"
    )
}

struct SqlCellFunctionParser<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> SqlCellFunctionParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn is_eof(&self) -> bool {
        self.index >= self.input.len()
    }

    fn skip_ws(&mut self) {
        let bytes = self.input.as_bytes();
        while self.index < bytes.len() && (bytes[self.index] as char).is_ascii_whitespace() {
            self.index += 1;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.input
            .as_bytes()
            .get(self.index)
            .map(|byte| *byte as char)
    }

    fn parse_identifier(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.index;
        let first = *bytes.get(self.index)? as char;
        if !first.is_ascii_alphabetic() && first != '_' {
            return None;
        }
        self.index += 1;
        while self.index < bytes.len() {
            let value = bytes[self.index] as char;
            if value.is_ascii_alphanumeric() || value == '_' {
                self.index += 1;
            } else {
                break;
            }
        }
        Some(self.input[start..self.index].to_ascii_uppercase())
    }

    fn parse_string_literal(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let quote = self.peek_char()?;
        if quote != '\'' && quote != '"' {
            return None;
        }
        let start = self.index;
        self.index += 1;
        while self.index < bytes.len() {
            let value = bytes[self.index] as char;
            if value == '\\' {
                self.index = (self.index + 2).min(bytes.len());
                continue;
            }
            self.index += 1;
            if value == quote {
                if self.index < bytes.len() && (bytes[self.index] as char) == quote {
                    self.index += 1;
                    continue;
                }
                return Some(self.input[start..self.index].to_string());
            }
        }
        self.index = start;
        None
    }

    fn parse_number_literal(&mut self) -> Option<String> {
        self.skip_ws();
        let bytes = self.input.as_bytes();
        let start = self.index;

        if matches!(self.peek_char(), Some('+') | Some('-')) {
            self.index += 1;
        }

        let mut has_digit = false;
        while self.index < bytes.len() {
            let value = bytes[self.index] as char;
            if value.is_ascii_digit() {
                has_digit = true;
                self.index += 1;
            } else {
                break;
            }
        }

        if self.index < bytes.len() && (bytes[self.index] as char) == '.' {
            self.index += 1;
            while self.index < bytes.len() {
                let value = bytes[self.index] as char;
                if value.is_ascii_digit() {
                    has_digit = true;
                    self.index += 1;
                } else {
                    break;
                }
            }
        }

        if self.index < bytes.len() {
            let value = bytes[self.index] as char;
            if value == 'e' || value == 'E' {
                self.index += 1;
                if self.index < bytes.len() {
                    let sign = bytes[self.index] as char;
                    if sign == '+' || sign == '-' {
                        self.index += 1;
                    }
                }
                let exp_start = self.index;
                while self.index < bytes.len() {
                    let digit = bytes[self.index] as char;
                    if digit.is_ascii_digit() {
                        self.index += 1;
                    } else {
                        break;
                    }
                }
                if self.index == exp_start {
                    self.index = start;
                    return None;
                }
            }
        }

        if !has_digit {
            self.index = start;
            return None;
        }

        Some(self.input[start..self.index].to_string())
    }

    fn parse_keyword_literal(&mut self) -> Option<String> {
        let start = self.index;
        let ident = self.parse_identifier()?;
        if ident == "NULL" || ident == "TRUE" || ident == "FALSE" {
            Some(ident)
        } else {
            self.index = start;
            None
        }
    }

    fn parse_argument(&mut self) -> Option<String> {
        if let Some(value) = self.parse_string_literal() {
            return Some(value);
        }
        if let Some(value) = self.parse_number_literal() {
            return Some(value);
        }
        let start = self.index;
        if let Some(value) = self.parse_expression() {
            return Some(value);
        }
        self.index = start;
        self.parse_keyword_literal()
    }

    fn parse_expression(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.index;
        let ident = self.parse_identifier()?;
        self.skip_ws();

        let has_call = matches!(self.peek_char(), Some('('));
        if !has_call {
            if ident == "CURRENT_TIMESTAMP" {
                return Some(String::from("CURRENT_TIMESTAMP"));
            }
            self.index = start;
            return None;
        }

        if !is_supported_sql_cell_function(&ident) {
            self.index = start;
            return None;
        }

        self.index += 1;
        let mut args = Vec::new();
        loop {
            self.skip_ws();
            if matches!(self.peek_char(), Some(')')) {
                self.index += 1;
                break;
            }
            let argument = self.parse_argument()?;
            args.push(argument);
            self.skip_ws();
            match self.peek_char() {
                Some(',') => {
                    self.index += 1;
                }
                Some(')') => {
                    self.index += 1;
                    break;
                }
                _ => {
                    self.index = start;
                    return None;
                }
            }
        }

        if ident == "NOW" && args.is_empty() {
            return Some(String::from("CURRENT_TIMESTAMP"));
        }
        if ident == "CURRENT_TIMESTAMP" && args.is_empty() {
            return Some(String::from("CURRENT_TIMESTAMP"));
        }

        Some(format!("{}({})", ident, args.join(", ")))
    }
}

pub(crate) fn sql_cell_function_expression(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parser = SqlCellFunctionParser::new(trimmed);
    let expression = parser.parse_expression()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return None;
    }
    Some(expression)
}

fn quote_sql_string(value: &str) -> String {
    sql_quote_string_literal(DatabaseDriver::MySql, value)
}

pub(crate) fn sql_literal_from_value_for_driver(
    value: &str,
    kind: ColumnKind,
    driver: DatabaseDriver,
) -> String {
    if is_null_value(value) {
        return String::from("NULL");
    }
    if let Some(expression) = sql_cell_function_expression(value) {
        return expression;
    }

    match kind {
        ColumnKind::Text => sql_quote_string_literal(driver, value),
        ColumnKind::Binary => {
            if value == "<0 bytes>" {
                return match driver {
                    DatabaseDriver::MySql | DatabaseDriver::MariaDb => String::from("''"),
                    DatabaseDriver::Sqlite => String::from("X''"),
                    DatabaseDriver::PostgreSql => String::from("'\\x'::bytea"),
                };
            }
            if is_binary_preview_value(value) {
                if let Some(space_index) = value.find(' ') {
                    let hex = &value[2..space_index];
                    if !hex.contains("...") {
                        return match driver {
                            DatabaseDriver::MySql | DatabaseDriver::MariaDb => {
                                format!("0x{}", hex)
                            }
                            DatabaseDriver::Sqlite => format!("X'{}'", hex),
                            DatabaseDriver::PostgreSql => {
                                format!("'\\x{}'::bytea", hex.to_ascii_lowercase())
                            }
                        };
                    }
                }
                return String::from("NULL");
            }
            sql_quote_string_literal(driver, value)
        }
        ColumnKind::Integer | ColumnKind::Unsigned | ColumnKind::Float | ColumnKind::Decimal => {
            value.to_string()
        }
        ColumnKind::Bool => {
            if value.eq_ignore_ascii_case("true") {
                match driver {
                    DatabaseDriver::PostgreSql => String::from("TRUE"),
                    _ => String::from("1"),
                }
            } else if value.eq_ignore_ascii_case("false") {
                match driver {
                    DatabaseDriver::PostgreSql => String::from("FALSE"),
                    _ => String::from("0"),
                }
            } else {
                value.to_string()
            }
        }
        ColumnKind::DateTime | ColumnKind::Date | ColumnKind::Time => {
            sql_quote_string_literal(driver, value)
        }
        ColumnKind::Unknown => sql_quote_string_literal(driver, value),
    }
}

pub(crate) fn sql_literal_from_row(row: &MySqlRow, index: usize, kind: ColumnKind) -> String {
    match kind {
        ColumnKind::Text => match row.try_get_unchecked::<Option<&str>, _>(index) {
            Ok(Some(value)) => quote_sql_string(value),
            Ok(None) => String::from("NULL"),
            Err(_) => match row.try_get_unchecked::<Option<&[u8]>, _>(index) {
                Ok(Some(bytes)) => quote_sql_string(&String::from_utf8_lossy(bytes)),
                Ok(None) => String::from("NULL"),
                Err(_) => {
                    let fallback = value_to_string_fallback(row, index);
                    if is_null_value(&fallback) {
                        String::from("NULL")
                    } else {
                        quote_sql_string(&fallback)
                    }
                }
            },
        },
        ColumnKind::Binary => match row.try_get_unchecked::<Option<&[u8]>, _>(index) {
            Ok(Some(bytes)) => format!("0x{}", bytes_to_hex(bytes)),
            Ok(None) => String::from("NULL"),
            Err(_) => {
                let fallback = value_to_string_fallback(row, index);
                if is_null_value(&fallback) {
                    String::from("NULL")
                } else {
                    quote_sql_string(&fallback)
                }
            }
        },
        ColumnKind::Integer => match row.try_get_unchecked::<Option<i64>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
        ColumnKind::Unsigned => match row.try_get_unchecked::<Option<u64>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
        ColumnKind::Float => match row.try_get_unchecked::<Option<f64>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
        ColumnKind::Decimal => match row.try_get_unchecked::<Option<BigDecimal>, _>(index) {
            Ok(Some(value)) => value.to_string(),
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
        ColumnKind::Bool => match row.try_get_unchecked::<Option<bool>, _>(index) {
            Ok(Some(value)) => {
                if value {
                    String::from("1")
                } else {
                    String::from("0")
                }
            }
            Ok(None) => String::from("NULL"),
            Err(_) => value_to_string_fallback(row, index),
        },
        ColumnKind::DateTime => match row.try_get_unchecked::<Option<NaiveDateTime>, _>(index) {
            Ok(Some(value)) => quote_sql_string(&value.format("%Y-%m-%d %H:%M:%S").to_string()),
            Ok(None) => String::from("NULL"),
            Err(_) => match row.try_get_unchecked::<Option<DateTime<Utc>>, _>(index) {
                Ok(Some(value)) => quote_sql_string(&value.format("%Y-%m-%d %H:%M:%S").to_string()),
                Ok(None) => String::from("NULL"),
                Err(_) => {
                    let fallback = value_to_string_fallback(row, index);
                    if is_null_value(&fallback) {
                        String::from("NULL")
                    } else {
                        quote_sql_string(&fallback)
                    }
                }
            },
        },
        ColumnKind::Date => match row.try_get_unchecked::<Option<NaiveDate>, _>(index) {
            Ok(Some(value)) => quote_sql_string(&value.format("%Y-%m-%d").to_string()),
            Ok(None) => String::from("NULL"),
            Err(_) => {
                let fallback = value_to_string_fallback(row, index);
                if is_null_value(&fallback) {
                    String::from("NULL")
                } else {
                    quote_sql_string(&fallback)
                }
            }
        },
        ColumnKind::Time => match row.try_get_unchecked::<Option<NaiveTime>, _>(index) {
            Ok(Some(value)) => quote_sql_string(&value.format("%H:%M:%S").to_string()),
            Ok(None) => String::from("NULL"),
            Err(_) => {
                let fallback = value_to_string_fallback(row, index);
                if is_null_value(&fallback) {
                    String::from("NULL")
                } else {
                    quote_sql_string(&fallback)
                }
            }
        },
        ColumnKind::Unknown => {
            let fallback = value_to_string_fallback(row, index);
            if is_null_value(&fallback) {
                String::from("NULL")
            } else {
                quote_sql_string(&fallback)
            }
        }
    }
}
