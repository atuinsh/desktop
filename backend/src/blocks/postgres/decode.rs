use eyre::{eyre, Result};
use serde_json::Value as JsonValue;
use sqlx::postgres::types::{Oid, PgInterval};
use sqlx::{postgres::PgValueRef, TypeInfo, Value, ValueRef};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};

pub(crate) fn to_json(v: PgValueRef) -> Result<JsonValue> {
    if v.is_null() {
        return Ok(JsonValue::Null);
    }

    let res = match v.type_info().name() {
        // Weirdly we get "CHAR" as the type, quoted, sometimes.
        "CHAR" | "VARCHAR" | "TEXT" | "NAME" | "citext" | "\"CHAR\"" => {
            match ValueRef::to_owned(&v).try_decode() {
                Ok(v) => JsonValue::String(v),
                _ => JsonValue::Null,
            }
        }
        "FLOAT4" => match ValueRef::to_owned(&v).try_decode::<f32>() {
            Ok(v) => JsonValue::from(v),
            _ => JsonValue::Null,
        },
        "FLOAT8" => match ValueRef::to_owned(&v).try_decode::<f64>() {
            Ok(v) => JsonValue::from(v),
            _ => JsonValue::Null,
        },
        "INT2" => match ValueRef::to_owned(&v).try_decode::<i16>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "INT4" => match ValueRef::to_owned(&v).try_decode::<i32>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "INT8" => match ValueRef::to_owned(&v).try_decode::<i64>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "BOOL" => match ValueRef::to_owned(&v).try_decode() {
            Ok(v) => JsonValue::Bool(v),
            _ => JsonValue::Null,
        },
        "DATE" => match ValueRef::to_owned(&v).try_decode::<Date>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "TIME" => match ValueRef::to_owned(&v).try_decode::<Time>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "TIMESTAMP" => match ValueRef::to_owned(&v).try_decode::<PrimitiveDateTime>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "TIMESTAMPTZ" => match ValueRef::to_owned(&v).try_decode::<OffsetDateTime>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "JSON" | "JSONB" => ValueRef::to_owned(&v).try_decode().unwrap_or_default(),
        "BYTEA" => match ValueRef::to_owned(&v).try_decode::<Vec<u8>>() {
            Ok(v) => JsonValue::Array(v.into_iter().map(|n| JsonValue::Number(n.into())).collect()),
            _ => JsonValue::Null,
        },
        "VOID" => JsonValue::Null,
        "UUID" => match ValueRef::to_owned(&v).try_decode::<uuid::Uuid>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "INTERVAL" => {
            match ValueRef::to_owned(&v).try_decode::<PgInterval>() {
                Ok(v) => {
                    // TODO: Figure out how many seconds are in a month? wtf postgres?
                    // Ok note to future me, but it looks like postgres just assumes a month has 30
                    // days? bruh?
                    // https://github.com/postgres/postgres/blob/9ca67658d19e6c258eb4021a326ed7d38b3ab75f/src/include/datatype/timestamp.h#L116
                    let _months = v.months;

                    let days_as_secs = i64::from(v.days)
                        .checked_mul(24 * 60 * 60)
                        .expect("pginterval day overflow"); // 24 hours * 60 minutes * 60 seconds

                    // Convert microseconds to seconds and nanoseconds
                    let micros_as_secs = v.microseconds.div_euclid(1_000_000);
                    let remaining_nanos = (v.microseconds.rem_euclid(1_000_000) * 1000) as u32;

                    // Combine all seconds
                    let total_secs = days_as_secs
                        .checked_add(micros_as_secs)
                        .expect("pginterval day/second overflow");

                    let duration = std::time::Duration::new(total_secs as u64, remaining_nanos);

                    JsonValue::String(humantime::format_duration(duration).to_string())
                }
                _ => JsonValue::Null,
            }
        }
        "OID" => match ValueRef::to_owned(&v).try_decode::<Oid>() {
            Ok(v) => JsonValue::Number(v.0.into()),
            _ => JsonValue::Null,
        },
        "NUMERIC" => {
            // Numeric is a bigdecimal, ie very high precision. It is likely used for things like financial data.
            // If we cast it to an integer or a float, we lose precision.
            // Seeing a we are using JSON (kinda), we do not have built-in bigdecimal support. So, for now at least,
            // we just convert it to a string.
            match ValueRef::to_owned(&v).try_decode::<bigdecimal::BigDecimal>() {
                Ok(v) => JsonValue::from(v.to_string()),
                _ => JsonValue::Null,
            }
        }
        _ => {
            return Err(eyre!(
                "Unsupported data type: {}",
                v.type_info().name().to_string()
            ))
        }
    };

    Ok(res)
}
