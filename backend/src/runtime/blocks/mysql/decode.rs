use eyre::{eyre, Result};
use serde_json::Value as JsonValue;
use sqlx::{mysql::MySqlValueRef, TypeInfo, Value, ValueRef};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};

// TODO(ellie): fix this one lol
// ref: https://github.com/launchbadge/sqlx/issues/3387
fn is_likely_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }

    let printable_count = bytes
        .iter()
        .filter(|&&b| {
            (32..=126).contains(&b) || (160..=255).contains(&b) || b == 9 || b == 10 || b == 13
        })
        .count();

    (printable_count as f32 / bytes.len() as f32) > 0.9
}

pub(crate) fn to_json(v: MySqlValueRef) -> Result<JsonValue> {
    if v.is_null() {
        return Ok(JsonValue::Null);
    }

    let res = match v.type_info().name() {
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" => {
            match ValueRef::to_owned(&v).try_decode::<String>() {
                Ok(v) => JsonValue::String(v),
                _ => JsonValue::Null,
            }
        }
        // Handle the SQLx bug where VARCHAR with binary collation shows as VARBINARY
        "VARBINARY" | "BINARY" => {
            match ValueRef::to_owned(&v).try_decode::<Vec<u8>>() {
                Ok(bytes) => {
                    // Try to detect if it's actually text
                    if is_likely_text(&bytes) {
                        // Convert as latin1
                        let text: String = bytes.iter().map(|&b| b as char).collect();
                        JsonValue::String(text)
                    } else {
                        // Actually binary data
                        JsonValue::Array(
                            bytes
                                .into_iter()
                                .map(|n| JsonValue::Number(n.into()))
                                .collect(),
                        )
                    }
                }
                _ => JsonValue::Null,
            }
        }
        "FLOAT" => match ValueRef::to_owned(&v).try_decode::<f32>() {
            Ok(v) => JsonValue::from(v),
            _ => JsonValue::Null,
        },
        "DOUBLE" => match ValueRef::to_owned(&v).try_decode::<f64>() {
            Ok(v) => JsonValue::from(v),
            _ => JsonValue::Null,
        },
        "TINYINT" => match ValueRef::to_owned(&v).try_decode::<i8>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "SMALLINT" => match ValueRef::to_owned(&v).try_decode::<i16>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "INT" | "MEDIUMINT" => match ValueRef::to_owned(&v).try_decode::<i32>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "BIGINT" => match ValueRef::to_owned(&v).try_decode::<i64>() {
            Ok(v) => JsonValue::Number(v.into()),
            _ => JsonValue::Null,
        },
        "BOOLEAN" => match ValueRef::to_owned(&v).try_decode::<bool>() {
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
        "DATETIME" => match ValueRef::to_owned(&v).try_decode::<PrimitiveDateTime>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "TIMESTAMP" => match ValueRef::to_owned(&v).try_decode::<OffsetDateTime>() {
            Ok(v) => JsonValue::String(v.to_string()),
            _ => JsonValue::Null,
        },
        "JSON" => ValueRef::to_owned(&v).try_decode().unwrap_or_default(),
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" => {
            match ValueRef::to_owned(&v).try_decode::<Vec<u8>>() {
                Ok(v) => {
                    JsonValue::Array(v.into_iter().map(|n| JsonValue::Number(n.into())).collect())
                }
                _ => JsonValue::Null,
            }
        }
        "DECIMAL" | "NUMERIC" => {
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
