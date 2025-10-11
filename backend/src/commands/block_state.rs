use std::collections::HashMap;

/// Set a local state property for a specific block
///
/// This stores the property in SQLite so it persists across reloads
/// but remains local to the user (not synced)
///
/// Returns true if the property was changed, false if it was already set to the same value
#[tauri::command]
pub async fn set_block_local_state(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
    property_name: String,
    property_value: String,
) -> Result<bool, String> {
    let pool = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    // Check if the property already exists with the same value
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT property_value FROM block_local_state 
         WHERE runbook_id = ? AND block_id = ? AND property_name = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .bind(&property_name)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;

    if let Some((existing_value,)) = existing {
        if existing_value == property_value {
            return Ok(false);
        }
    }

    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    // Insert or update the property
    sqlx::query(
        "INSERT INTO block_local_state 
         (runbook_id, block_id, property_name, property_value, created, updated)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(runbook_id, block_id, property_name) 
         DO UPDATE SET property_value = ?, updated = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .bind(&property_name)
    .bind(&property_value)
    .bind(now)
    .bind(now)
    .bind(&property_value)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(true)
}

/// Get a local state property for a specific block
///
/// Returns None if the property doesn't exist
#[tauri::command]
pub async fn get_block_local_state(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
    property_name: String,
) -> Result<Option<String>, String> {
    let pool = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    let result: Option<(String,)> = sqlx::query_as(
        "SELECT property_value FROM block_local_state 
         WHERE runbook_id = ? AND block_id = ? AND property_name = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .bind(&property_name)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.map(|(value,)| value))
}

/// Get all local state properties for a specific block
///
/// Returns a map of property names to values
#[tauri::command]
pub async fn get_block_local_state_all(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
) -> Result<HashMap<String, String>, String> {
    let pool = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    let results: Vec<(String, String)> = sqlx::query_as(
        "SELECT property_name, property_value FROM block_local_state 
         WHERE runbook_id = ? AND block_id = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(results.into_iter().collect())
}

/// Delete a local state property for a specific block
///
/// Returns true if the property was deleted, false if it didn't exist
#[tauri::command]
pub async fn delete_block_local_state(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
    property_name: String,
) -> Result<bool, String> {
    let pool = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    let result = sqlx::query(
        "DELETE FROM block_local_state 
         WHERE runbook_id = ? AND block_id = ? AND property_name = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .bind(&property_name)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.rows_affected() > 0)
}

/// Delete all local state properties for a specific block
///
/// Returns the number of properties deleted
#[tauri::command]
pub async fn delete_block_local_state_all(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook_id: String,
    block_id: String,
) -> Result<u64, String> {
    let pool = state
        .db_instances
        .get_pool("runbooks")
        .await
        .map_err(|e| e.to_string())?;

    let result = sqlx::query(
        "DELETE FROM block_local_state 
         WHERE runbook_id = ? AND block_id = ?",
    )
    .bind(&runbook_id)
    .bind(&block_id)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.rows_affected())
}
