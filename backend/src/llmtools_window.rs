use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl};

/// Creates the LLM Tools window if it doesn't exist, or focuses it if it does.
pub(crate) fn create_llmtools_window<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    // Find the main window
    let main_window = match app.get_webview_window("main") {
        Some(w) => w,
        None => return Err("Main window not found".into()),
    };
    // Return early if devtools are not open on the main window
    if !main_window.is_devtools_open() {
        return Ok(());
    }

    // If window exists, focus it and return
    if let Some(window) = app.get_webview_window("llmtools") {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "llmtools", WebviewUrl::App("llmtools.html".into()))
        .title("LLM Tools")
        .inner_size(900.0, 600.0)
        .min_inner_size(400.0, 300.0)
        .resizable(true)
        .visible(true)
        .always_on_top(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}
