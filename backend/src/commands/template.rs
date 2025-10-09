use std::collections::HashMap;

/// Set a template variable for a runbook
///
/// This stores the variable in the state so it can be accessed by the template engine
///
/// Returns true if the variable was changed, false if it was already set to the same value
#[tauri::command]
pub async fn set_template_var(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook: String,
    name: String,
    value: String,
) -> Result<bool, String> {
    // Store the variable in the state
    let mut vars = state.runbook_output_variables.write().await;
    let entry = vars.entry(runbook).or_insert(HashMap::new());

    if entry.get(&name).map(|v| v == &value).unwrap_or(false) {
        return Ok(false);
    }

    entry.insert(name, value);

    Ok(true)
}

/// Get a template variable for a runbook
///
/// This retrieves the variable from the state so it can be accessed by the frontend
#[tauri::command]
pub async fn get_template_var(
    state: tauri::State<'_, crate::state::AtuinState>,
    runbook: String,
    name: String,
) -> Result<Option<String>, String> {
    // Get the variable from the state
    let value = state
        .runbook_output_variables
        .read()
        .await
        .get(&runbook)
        .and_then(|vars| vars.get(&name))
        .cloned();

    Ok(value)
}
