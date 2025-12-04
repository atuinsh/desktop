use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone)]
#[ts(export)]
pub struct AdvancedSettings {
    /// Whether to copy the login shell environment to the app's environment.
    pub copy_shell_env: bool,
}

impl AdvancedSettings {
    pub(crate) fn load(config_path: &Path) -> eyre::Result<Self> {
        config::Config::builder()
            .add_source(
                config::File::with_name(config_path.to_str().ok_or(eyre::eyre!(
                    "Failed to convert config path '{path}' to string",
                    path = config_path.display()
                ))?)
                .required(false),
            )
            .set_default("copy_shell_env", true)?
            .build()?
            .try_deserialize::<Self>()
            .map_err(|e| eyre::eyre!("Failed to deserialize advanced settings: {}", e))
    }
}

#[tauri::command]
pub async fn get_advanced_settings(
    state: tauri::State<'_, AdvancedSettings>,
) -> Result<AdvancedSettings, String> {
    let advanced_settings = state.inner().clone();
    Ok(advanced_settings)
}
