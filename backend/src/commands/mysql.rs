use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{Column, Connection, MySqlConnection, Row};

#[derive(Debug, Default, Serialize, Deserialize)]
struct MySqlColumn {
    name: String,
    type_: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MySqlQueryResult {
    columns: Vec<MySqlColumn>,
    rows: Vec<Vec<JsonValue>>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MySqlExecuteResult {
    rows_affected: u64,
}

#[tauri::command]
pub async fn mysql_query(
    uri: String,
    query: String,
    values: Option<Vec<JsonValue>>,
) -> Result<MySqlQueryResult, String> {
    let mut conn = MySqlConnection::connect(uri.as_str())
        .await
        .map_err(|e| e.to_string())?;

    let mut query = sqlx::query(&query);
    for value in values.unwrap_or_default() {
        if value.is_null() {
            query = query.bind(None::<JsonValue>);
        } else if value.is_string() {
            query = query.bind(value.as_str().unwrap().to_owned())
        } else {
            query = query.bind(value);
        }
    }
    let rows = query
        .fetch_all(&mut conn)
        .await
        .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Ok(MySqlQueryResult::default());
    }

    let columns = rows[0]
        .columns()
        .iter()
        .map(|c| MySqlColumn {
            name: c.name().to_owned(),
            type_: c.type_info().to_string(),
        })
        .collect();

    let mut values = Vec::new();

    for row in rows {
        let mut value = Vec::default();

        for (i, _) in row.columns().iter().enumerate() {
            let v = row.try_get_raw(i).map_err(|e| e.to_string())?;
            let v = atuin_desktop_runtime::mysql_decode::to_json(v).map_err(|e| e.to_string())?;

            value.push(v);
        }

        values.push(value);
    }

    Ok(MySqlQueryResult {
        rows: values,
        columns,
    })
}

#[tauri::command]
pub async fn mysql_execute(
    uri: String,
    query: String,
    values: Option<Vec<JsonValue>>,
) -> Result<MySqlExecuteResult, String> {
    let mut conn = MySqlConnection::connect(uri.as_str())
        .await
        .map_err(|e| e.to_string())?;

    let mut query = sqlx::query(&query);
    for value in values.unwrap_or_default() {
        if value.is_null() {
            query = query.bind(None::<JsonValue>);
        } else if value.is_string() {
            query = query.bind(value.as_str().unwrap().to_owned())
        } else {
            query = query.bind(value);
        }
    }
    let res = query.execute(&mut conn).await.map_err(|e| e.to_string())?;

    Ok(MySqlExecuteResult {
        rows_affected: res.rows_affected(),
    })
}
