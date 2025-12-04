use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdvancedSettings {
    /// Whether to copy the login shell environment to the app's environment.
    #[serde(default = "default_copy_shell_env")]
    pub copy_shell_env: bool,
}

fn default_copy_shell_env() -> bool {
    true
}

#[tauri::command]
pub async fn get_advanced_settings(
    state: tauri::State<'_, AdvancedSettings>,
) -> Result<AdvancedSettings, String> {
    let advanced_settings = state.inner().clone();
    Ok(advanced_settings)
}
