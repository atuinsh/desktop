#[tauri::command]
pub async fn check_binary_exists(path: String) -> Result<bool, String> {
    // Check if the binary exists and is executable
    let path = shellexpand::tilde(&path).to_string();
    let exists = tokio::fs::metadata(&path).await.map(|meta| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            meta.is_file() && meta.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            meta.is_file()
        }
    });

    Ok(exists.unwrap_or(false))
}
