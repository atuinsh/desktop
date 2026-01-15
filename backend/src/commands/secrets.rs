use crate::state::AtuinState;

#[tauri::command]
pub async fn save_password(
    state: tauri::State<'_, AtuinState>,
    service: &str,
    user: &str,
    value: &str,
) -> Result<(), String> {
    log::info!("save_password for {service}, {user}");

    state
        .secret_cache()
        .set(service, user, value)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn load_password(
    state: tauri::State<'_, AtuinState>,
    service: &str,
    user: &str,
) -> Result<Option<String>, String> {
    log::info!("load_password for {service}, {user}");

    let secret = state
        .secret_cache()
        .get(service, user)
        .await
        .map_err(|e| e.to_string())?;

    Ok(secret)
}

#[tauri::command]
pub async fn delete_password(
    state: tauri::State<'_, AtuinState>,
    service: &str,
    user: &str,
) -> Result<(), String> {
    log::info!("delete_password for {service}, {user}");

    state
        .secret_cache()
        .delete(service, user)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
