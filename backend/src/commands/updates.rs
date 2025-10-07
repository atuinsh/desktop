use std::time::Duration;

use serde::Serialize;
use tauri::{Manager, ResourceId, Runtime, Url, Webview};
use tauri_plugin_updater::UpdaterExt;

// This file is largely copied from the tauri-plugin-updater crate,
// as they don't export all the structs we need to use.

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata {
    rid: ResourceId,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

#[tauri::command]
pub(crate) async fn check_for_updates<R: Runtime>(
    webview: Webview<R>,
    headers: Option<Vec<(String, String)>>,
    timeout: Option<u64>,
    proxy: Option<String>,
    target: Option<String>,
    allow_downgrades: Option<bool>,
) -> Result<Option<Metadata>, String> {
    let update_channel = env!("APP_CHANNEL");

    let update_endpoints = if update_channel == "edge" {
        vec![format!("https://hub.atuin.sh/api/updates/{update_channel}/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}")]
    } else {
        vec![
            "https://releases.atuin.sh/{{target}}/{{arch}}/{{current_version}}".to_string(),
            "https://github.com/atuinsh/desktop/releases/latest/download/latest.json".to_string(),
            "https://cdn.crabnebula.app/update/atuin/atuin-desktop/{{target}}-{{arch}}/{{current_version}}".to_string(),
        ]
    };

    let mut builder = webview.updater_builder();
    builder = builder
        .endpoints(
            update_endpoints
                .into_iter()
                .map(|s| Url::parse(&s).unwrap())
                .collect::<Vec<Url>>(),
        )
        .map_err(|e| e.to_string())?;

    if let Some(headers) = headers {
        for (k, v) in headers {
            builder = builder.header(k, v).map_err(|e| e.to_string())?;
        }
    }
    if let Some(timeout) = timeout {
        builder = builder.timeout(Duration::from_millis(timeout));
    }
    if let Some(ref proxy) = proxy {
        let url = Url::parse(proxy.as_str()).map_err(|e| e.to_string())?;
        builder = builder.proxy(url);
    }
    if let Some(target) = target {
        builder = builder.target(target);
    }
    if allow_downgrades.unwrap_or(false) || update_channel == "edge" {
        builder = builder.version_comparator(|current, update| update.version != current);
    }

    let updater = builder.build().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;

    if let Some(update) = update {
        let formatted_date = if let Some(date) = update.date {
            let formatted_date = date
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| "Failed to format date".to_string())?;
            Some(formatted_date)
        } else {
            None
        };
        let metadata = Metadata {
            current_version: update.current_version.clone(),
            version: update.version.clone(),
            date: formatted_date,
            body: update.body.clone(),
            raw_json: update.raw_json.clone(),
            rid: webview.resources_table().add(update),
        };
        Ok(Some(metadata))
    } else {
        Ok(None)
    }
}
