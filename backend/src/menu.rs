use std::fmt::Display;

use eyre::Result;
use serde::Deserialize;
use tauri::{
    menu::{
        AboutMetadata, IsMenuItem, Menu, MenuItem, MenuItemBuilder, PredefinedMenuItem, Submenu,
    },
    AppHandle, Emitter, Manager, Runtime,
};

struct IdWithNoColons(String);

impl IdWithNoColons {
    pub fn new(id: String) -> Result<Self> {
        if id.contains(":") {
            return Err(eyre::eyre!("ID cannot contain colons"));
        }
        Ok(Self(id))
    }
}

impl Display for IdWithNoColons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for IdWithNoColons {
    type Error = eyre::Error;
    fn try_from(id: String) -> Result<Self> {
        Self::new(id)
    }
}

impl TryFrom<&str> for IdWithNoColons {
    type Error = eyre::Error;
    fn try_from(id: &str) -> Result<Self> {
        Self::new(id.to_string())
    }
}

pub(crate) fn initialize_menu_handlers<R: Runtime>(handle: &AppHandle<R>) {
    handle.on_menu_event(move |app_handle, event| match event.id().0.as_str() {
        "update-check" => {
            app_handle
                .emit("update-check", 0)
                .expect("Failed to emit menu event");
        }
        "start-sync" => {
            app_handle
                .emit("start-sync", 0)
                .expect("Failed to emit menu event");
        }
        "import-runbook" => {
            app_handle
                .emit("import-runbook", 0)
                .expect("Failed to emit menu event");
        }
        "new-runbook" => {
            app_handle
                .emit("new-runbook", 0)
                .expect("Failed to emit menu event");
        }
        "new-workspace" => {
            app_handle
                .emit("new-workspace", 0)
                .expect("Failed to emit menu event");
        }
        "export-markdown" => {
            app_handle
                .emit("export-markdown", 0)
                .expect("Failed to emit menu event");
        }
        "toggle-devtools" => {
            let window = app_handle.get_webview_window("main").unwrap();
            if window.is_devtools_open() {
                window.close_devtools();
            } else {
                window.open_devtools();
            }
        }
        other_id if other_id.starts_with("link-menu-item:") => {
            let href = other_id.splitn(3, ":").nth(2);
            if let Some(href) = href {
                let _ = open::that(href);
            } else {
                log::warn!("Unknown menu event: {other_id}");
            }
        }
        other_id if other_id.starts_with("window-tab-item:") => {
            let url = other_id.split_once(":").map(|x| x.1);
            if let Some(url) = url {
                app_handle.emit("activate-tab", url).unwrap();
            } else {
                log::warn!("Unknown menu event: {other_id}");
            }
        }
        other_id => {
            log::warn!("Unknown menu event: {other_id}");
        }
    });
}

#[allow(dead_code)]
fn update_check<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let update_check = MenuItemBuilder::new("Check for Updates")
        .id("update-check")
        .build(handle)?;

    Ok(update_check)
}

#[allow(dead_code)]
fn start_sync<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let start_sync = MenuItemBuilder::new("Start Sync")
        .id("start-sync")
        .build(handle)?;

    Ok(start_sync)
}

#[allow(dead_code)]
fn import_runbook<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let import_runbook = MenuItemBuilder::new("Import Runbook")
        .id("import-runbook")
        .build(handle)?;

    Ok(import_runbook)
}

#[allow(dead_code)]
fn new_runbook<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let new_runbook = MenuItemBuilder::new("New Runbook")
        .id("new-runbook")
        .build(handle)?;

    Ok(new_runbook)
}

#[allow(dead_code)]
fn new_workspace<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let import_workspace = MenuItemBuilder::new("New Workspace")
        .id("new-workspace")
        .build(handle)?;

    Ok(import_workspace)
}

#[allow(dead_code)]
fn export_markdown<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let export_markdown = MenuItemBuilder::new("Markdown")
        .id("export-markdown")
        .build(handle)?;

    Ok(export_markdown)
}

#[allow(dead_code)]
fn show_devtools<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let show_devtools = MenuItemBuilder::new("Toggle DevTools")
        .id("toggle-devtools")
        .accelerator("CmdOrCtrl+Shift+I")
        .build(handle)?;

    Ok(show_devtools)
}

fn link_menu_item<R: Runtime>(
    id: IdWithNoColons,
    name: &str,
    href: &str,
    handle: &AppHandle<R>,
) -> Result<MenuItem<R>> {
    let id = format!("link-menu-item:{id}:{href}");
    let link = MenuItemBuilder::new(name).id(id).build(handle)?;

    Ok(link)
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TabItem {
    url: String,
    title: String,
}

pub fn menu<R: Runtime>(app_handle: &AppHandle<R>, tab_items: &[TabItem]) -> Result<Menu<R>> {
    // Totally just ripped the default menu from the Tauri source, and edited
    // Easier than screwing around with the API 🤫
    let pkg_info = app_handle.package_info();
    let config = app_handle.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    // Build tab menu items first
    let tab_menu_items: Vec<_> = tab_items
        .iter()
        .flat_map(|tab| {
            let app_handle = app_handle.clone();
            MenuItemBuilder::new(&tab.title)
                .id(format!("window-tab-item:{}", tab.url.clone()))
                .build(&app_handle)
                .ok()
        })
        .collect();

    // Create a vector of all window menu items
    let minimize = PredefinedMenuItem::minimize(app_handle, None)?;
    let maximize = PredefinedMenuItem::maximize(app_handle, None)?;
    let separator1 = PredefinedMenuItem::separator(app_handle)?;

    let mut window_items: Vec<&dyn IsMenuItem<R>> = vec![&minimize, &maximize];

    if !tab_menu_items.is_empty() {
        window_items.push(&separator1);
    }

    // Add tab menu items
    for item in tab_menu_items.iter() {
        window_items.push(item);
    }

    let window_menu =
        Submenu::with_id_and_items(app_handle, "window_menu", "Window", true, &window_items)?;

    let help_menu = Submenu::with_id_and_items(
        app_handle,
        "help_menu",
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(
                app_handle,
                Some("About Atuin Desktop"),
                Some(about_metadata),
            )?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app_handle)?,
            &link_menu_item(
                "twitter".try_into()?,
                "Atuin Twitter",
                "https://x.com/atuinsh",
                app_handle,
            )?,
            &link_menu_item(
                "mastodon".try_into()?,
                "Atuin Mastodon",
                "https://hachyderm.io/@atuin",
                app_handle,
            )?,
        ],
    )?;

    let menu = Menu::with_items(
        app_handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(
                        app_handle,
                        Some("About Atuin Desktop"),
                        Some(about_metadata),
                    )?,
                    &update_check(app_handle)?,
                    &start_sync(app_handle)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::services(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::hide(app_handle, Some("Hide Atuin Desktop"))?,
                    &PredefinedMenuItem::hide_others(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::quit(app_handle, Some("Quit Atuin Desktop"))?,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                app_handle,
                "File",
                true,
                &[
                    &Submenu::with_items(
                        app_handle,
                        "Runbooks",
                        true,
                        &[&new_runbook(app_handle)?, &import_runbook(app_handle)?],
                    )?,
                    &Submenu::with_items(
                        app_handle,
                        "Workspaces",
                        true,
                        &[&new_workspace(app_handle)?],
                    )?,
                    &Submenu::with_items(
                        app_handle,
                        "Export",
                        true,
                        &[&export_markdown(app_handle)?],
                    )?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::quit(app_handle, Some("Quit Atuin Desktop"))?,
                ],
            )?,
            &Submenu::with_items(
                app_handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app_handle, None)?,
                    &PredefinedMenuItem::redo(app_handle, None)?,
                    &PredefinedMenuItem::separator(app_handle)?,
                    &PredefinedMenuItem::cut(app_handle, None)?,
                    &PredefinedMenuItem::copy(app_handle, None)?,
                    &PredefinedMenuItem::paste(app_handle, None)?,
                    &PredefinedMenuItem::select_all(app_handle, None)?,
                ],
            )?,
            #[cfg(debug_assertions)]
            &Submenu::with_items(
                app_handle,
                "Developer",
                true,
                &[&show_devtools(app_handle)?],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app_handle,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app_handle, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )?;

    Ok(menu)
}
