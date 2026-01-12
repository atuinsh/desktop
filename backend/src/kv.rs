use serde::{de::DeserializeOwned, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) async fn get<T: DeserializeOwned>(
    db: &SqlitePool,
    key: &str,
) -> Result<Option<T>, String> {
    let res = sqlx::query("select value from kv where key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string());

    match res {
        Ok(Some(row)) => {
            let value_str: String = row.get("value");
            serde_json::from_str(&value_str)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

pub(crate) async fn set<T: Serialize>(db: &SqlitePool, key: &str, value: &T) -> Result<(), String> {
    sqlx::query("insert or replace into kv(key, value) values($1, $2)")
        .bind(key)
        .bind(serde_json::to_string(value).map_err(|e| e.to_string())?)
        .execute(db)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(crate) async fn open_db<R: Runtime>(app: &AppHandle<R>) -> eyre::Result<SqlitePool> {
    let state = app.state::<crate::state::AtuinState>();
    state.db_instances.get_pool("kv").await
}
