use sqlx::Row;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, Runtime};

#[tauri::command]
pub async fn save_ydoc_for_runbook<R: Runtime>(
    app: AppHandle<R>,
    request: Request<'_>,
) -> Result<(), String> {
    if let InvokeBody::Raw(data) = request.body() {
        let runbook_id = request.headers().get("id").unwrap().to_str().unwrap();

        let state = app.state::<crate::state::AtuinState>();
        let db = state
            .db_instances
            .get_pool("runbooks")
            .await
            .map_err(|e| e.to_string())?;

        sqlx::query("update runbooks set ydoc = ?1 where id = ?2")
            .bind(data)
            .bind(runbook_id)
            .execute(&db)
            .await
            .unwrap();

        Ok(())
    } else {
        Err("Something went wrong".to_string())
    }
}

#[tauri::command]
pub async fn load_ydoc_for_runbook<R: Runtime>(
    app: AppHandle<R>,
    runbook_id: &str,
) -> Result<Response, String> {
    let state = app.state::<crate::state::AtuinState>();
    let db = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    let result = sqlx::query("select ydoc from runbooks where id = ?1")
        .bind(runbook_id)
        .fetch_one(&db)
        .await
        .unwrap();

    let data = result.get::<Vec<_>, _>("ydoc");

    Ok(Response::new(data))
}
