use eyre::Result;
use serde::Deserialize;
use tauri::{
    menu::{
        AboutMetadata, IsMenuItem, Menu, MenuItem, MenuItemBuilder, PredefinedMenuItem, Submenu,
    },
    AppHandle, Emitter, Manager, Runtime,
};

#[allow(dead_code)]
fn update_check<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let update_check = MenuItemBuilder::new("Check for Updates")
        .id("update-check")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "update-check" {
            window
                .emit("update-check", 0)
                .expect("Failed to emit menu event");
        }
    });

    Ok(update_check)
}

#[allow(dead_code)]
fn start_sync<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let start_sync = MenuItemBuilder::new("Start Sync")
        .id("start-sync")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "start-sync" {
            window
                .emit("start-sync", 0)
                .expect("Failed to emit menu event");
        }
    });

    Ok(start_sync)
}

#[allow(dead_code)]
fn import_runbook<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let import_runbook = MenuItemBuilder::new("Import Runbook")
        .id("import-runbook")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "import-runbook" {
            window
                .emit("import-runbook", 0)
                .expect("Failed to emit menu event");
        }
    });
    Ok(import_runbook)
}

#[allow(dead_code)]
fn new_runbook<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let new_runbook = MenuItemBuilder::new("New Runbook")
        .id("new-runbook")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "new-runbook" {
            window
                .emit("new-runbook", 0)
                .expect("Failed to emit menu event");
        }
    });
    Ok(new_runbook)
}

#[allow(dead_code)]
fn new_workspace<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let import_workspace = MenuItemBuilder::new("New Workspace")
        .id("new-workspace")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "new-workspace" {
            window
                .emit("new-workspace", 0)
                .expect("Failed to emit menu event");
        }
    });
    Ok(import_workspace)
}

#[allow(dead_code)]
fn export_markdown<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let export_markdown = MenuItemBuilder::new("Markdown")
        .id("export-markdown")
        .build(handle)?;

    handle.on_menu_event(move |window, event| {
        if event.id().0 == "export-markdown" {
            window
                .emit("export-markdown", 0)
                .expect("Failed to emit menu event");
        }
    });
    Ok(export_markdown)
}

#[allow(dead_code)]
fn show_devtools<R: Runtime>(handle: &AppHandle<R>) -> Result<MenuItem<R>> {
    let show_devtools = MenuItemBuilder::new("Toggle DevTools")
        .id("toggle-devtools")
        .accelerator("CmdOrCtrl+Shift+I")
        .build(handle)?;

    handle.on_menu_event(move |handle, event| {
        if event.id().0 == "toggle-devtools" {
            let window = handle.get_webview_window("main").unwrap();
            if window.is_devtools_open() {
                window.close_devtools();
            } else {
                window.open_devtools();
            }
        }
    });

    Ok(show_devtools)
}

fn link_menu_item<R: Runtime>(
    id: &str,
    name: &str,
    href: &str,
    handle: &AppHandle<R>,
) -> Result<MenuItem<R>> {
    let link = MenuItemBuilder::new(name).id(id).build(handle)?;

    let href = href.to_string();
    let id = id.to_string();

    handle.on_menu_event(move |_, event| {
        if event.id().0 == id {
            // Failing to open a link sucks but let's not error
            let _ = open::that(&href);
        }
    });

    Ok(link)
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TabItem {
    id: String,
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
        .map(|tab| {
            let app_handle = app_handle.clone();
            let item = MenuItemBuilder::new(&tab.title)
                .id(tab.id.clone())
                .build(&app_handle)
                .ok();

            let id = tab.id.clone();
            let url = tab.url.clone();
            let app_handle_clone = app_handle.clone();
            if let Some(item) = item {
                app_handle.on_menu_event(move |_, event| {
                    if event.id().0 == id {
                        app_handle_clone.emit("activate-tab", url.clone()).unwrap();
                    }
                });

                Some(item)
            } else {
                None
            }
        })
        .collect();

    // Create a vector of all window menu items
    let minimize = PredefinedMenuItem::minimize(app_handle, None)?;
    let maximize = PredefinedMenuItem::maximize(app_handle, None)?;
    #[cfg(target_os = "macos")]
    let separator1 = PredefinedMenuItem::separator(app_handle)?;

    let mut window_items: Vec<&dyn IsMenuItem<R>> = vec![&minimize, &maximize];

    #[cfg(target_os = "macos")]
    window_items.push(&separator1);

    // Add tab menu items
    for item in tab_menu_items.iter().flatten() {
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
                "twitter",
                "Atuin Twitter",
                "https://x.com/atuinsh",
                app_handle,
            )?,
            &link_menu_item(
                "mastodon",
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
