// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Issue to track: https://github.com/tauri-apps/tauri/issues/12382
#![allow(deprecated)]

use std::path::PathBuf;
use std::{env, fs};

use tauri::path::BaseDirectory;
use tauri::{http, App, AppHandle, Manager, RunEvent};
use tauri_plugin_log::fern::colors::{Color, ColoredLevelConfig};
use time::format_description::well_known::Rfc3339;

mod advanced_settings;
mod blocks;
mod db;
mod dotfiles;
mod file;
mod font;
mod install;
mod kv;
mod main_window;
mod menu;
mod run;
mod runbooks;
mod secret;
mod shared_state;
mod shellcheck;
mod sqlite;
mod state;
mod stats;
mod store;
mod util;
mod workspaces;

// If this works out ergonomically, we should move all the commands into a single module
// Separate the implementation from the command as much as we can
mod commands;

use atuin_client::settings::Settings;
use atuin_client::{history::HISTORY_TAG, record::sqlite_store::SqliteStore, record::store::Store};
use atuin_history::stats as atuin_stats;
use db::{GlobalStats, HistoryDB, UIHistory};
use dotfiles::aliases::aliases;

use crate::advanced_settings::AdvancedSettings;
use crate::menu::TabItem;

#[derive(Debug, serde::Serialize)]
struct HomeInfo {
    pub record_count: u64,
    pub history_count: u64,
    pub username: Option<String>,
    pub last_sync: Option<String>,
    pub top_commands: Vec<(String, u64)>,
    pub recent_commands: Vec<UIHistory>,
}

#[tauri::command]
async fn list(offset: Option<u64>) -> Result<Vec<UIHistory>, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let history = db
        .list(Some(offset.unwrap_or(0)), Some(100))
        .await?
        .into_iter()
        .map(|h| h.into())
        .collect();

    Ok(history)
}

#[tauri::command]
async fn search(query: String, offset: Option<u64>) -> Result<Vec<UIHistory>, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let history = db.search(offset, query.as_str()).await?;

    Ok(history)
}

#[tauri::command]
async fn global_stats() -> Result<GlobalStats, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;
    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let mut stats = db.global_stats().await?;

    let history = db.list(None, None).await?;
    let history_stats = atuin_stats::compute(&settings, &history, 10, 1);

    stats.stats = history_stats;

    Ok(stats)
}

#[tauri::command]
async fn config() -> Result<Settings, String> {
    Settings::new().map_err(|e| e.to_string())
}

#[tauri::command]
async fn session() -> Result<String, String> {
    Settings::new()
        .map_err(|e| e.to_string())?
        .session_token()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn login(username: String, password: String, key: String) -> Result<String, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    let record_store_path = PathBuf::from(settings.record_store_path.as_str());
    let store = SqliteStore::new(record_store_path, settings.local_timeout)
        .await
        .map_err(|e| e.to_string())?;

    if settings.logged_in() {
        return Err(String::from("Already logged in"));
    }

    let session = atuin_client::login::login(&settings, &store, username, password, key)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session)
}

#[tauri::command]
async fn logout() -> Result<(), String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    atuin_client::logout::logout(&settings).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
async fn register(username: String, email: String, password: String) -> Result<String, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    let session = atuin_client::register::register(&settings, username, email, password)
        .await
        .map_err(|e| e.to_string())?;

    Ok(session)
}

#[tauri::command]
async fn home_info() -> Result<HomeInfo, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;
    let record_store_path = PathBuf::from(settings.record_store_path.as_str());
    let sqlite_store = SqliteStore::new(record_store_path, settings.local_timeout)
        .await
        .map_err(|e| e.to_string())?;
    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let last_sync = Settings::last_sync()
        .map_err(|e| e.to_string())?
        .format(&Rfc3339)
        .map_err(|e| e.to_string())?;

    let record_count = sqlite_store.len_all().await.map_err(|e| e.to_string())?;
    let history_count = sqlite_store
        .len_tag(HISTORY_TAG)
        .await
        .map_err(|e| e.to_string())?;

    let history = db.list(None, None).await?;
    let stats = atuin_stats::compute(&settings, &history, 10, 1)
        .map_or(vec![], |stats| stats.top[0..5].to_vec())
        .iter()
        .map(|(commands, count)| (commands.join(" "), *count as u64))
        .collect();
    let recent = if history.len() > 5 {
        history[0..5].to_vec()
    } else {
        vec![]
    };
    let recent = recent.into_iter().map(|h| h.into()).collect();

    let info = if !settings.logged_in() {
        HomeInfo {
            username: None,
            last_sync: None,
            record_count,
            history_count,
            top_commands: stats,
            recent_commands: recent,
        }
    } else {
        let client = atuin_client::api_client::Client::new(
            &settings.sync_address,
            settings
                .session_token()
                .map_err(|e| e.to_string())?
                .as_str(),
            settings.network_connect_timeout,
            settings.network_timeout,
        )
        .map_err(|e| e.to_string())?;

        let me = client.me().await.map_err(|e| e.to_string())?;

        HomeInfo {
            username: Some(me.username),
            last_sync: Some(last_sync.to_string()),
            record_count,
            history_count,
            top_commands: stats,
            recent_commands: recent,
        }
    };

    Ok(info)
}

// Match the format that the frontend library we use expects
// All the processing in Rust, not JSunwrap.
// Faaaassssssst af ⚡️🦀
#[derive(Debug, serde::Serialize)]
pub struct HistoryCalendarDay {
    pub date: String,
    pub count: u64,
    pub level: u8,
}

#[tauri::command]
async fn history_calendar(
    command: Option<String>,
    path: Option<String>,
    hostname: Option<String>,
    start: Option<i64>,
    end: Option<i64>,
) -> Result<Vec<HistoryCalendarDay>, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;
    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let calendar = db
        .calendar(
            command,
            path,
            hostname,
            start.map(|s| s * 1000000),
            end.map(|e| e * 1000000),
        )
        .await?;

    // probs don't want to iterate _this_ many times, but it's only the last year. so 365
    // iterations at max. should be quick.

    // Handle case where calendar is empty (can happen on Linux with XDG dir issues)
    let default_entry = (String::new(), 0);
    let max = calendar
        .iter()
        .max_by_key(|d| d.1)
        .unwrap_or(&default_entry);

    let ret = calendar
        .iter()
        .map(|d| {
            // calculate the "level". we have 5, so figure out which 5th it fits into
            let percent: f64 = d.1 as f64 / max.1 as f64;
            let level = if d.1 == 0 {
                0.0
            } else {
                (percent / 0.2).round() + 1.0
            };

            HistoryCalendarDay {
                date: d.0.clone(),
                count: d.1,
                level: std::cmp::min(4, level as u8),
            }
        })
        .collect();

    Ok(ret)
}

#[tauri::command]
async fn prefix_search(query: &str) -> Result<Vec<String>, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;

    let db_path = PathBuf::from(settings.db_path.as_str());
    let db = HistoryDB::new(db_path, settings.local_timeout).await?;

    let history = db.prefix_search(query).await?;
    let commands = history.into_iter().map(|h| h.command).collect();

    Ok(commands)
}

#[tauri::command]
async fn cli_settings() -> Result<Settings, String> {
    let settings = Settings::new().map_err(|e| e.to_string())?;
    Ok(settings)
}

#[tauri::command]
async fn get_app_version(app: AppHandle) -> Result<String, String> {
    let version = app.package_info().version.to_string();
    Ok(version)
}

#[tauri::command]
async fn get_platform_info() -> Result<String, String> {
    let base_os = if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "linux") {
        // Try to get more detailed Linux distro info
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|contents| {
                contents
                    .lines()
                    .find(|line| line.starts_with("ID="))
                    .map(|id_line| {
                        let distro = id_line
                            .strip_prefix("ID=")
                            .unwrap_or("linux")
                            .trim_matches('"');
                        format!("linux-{distro}")
                    })
            })
            .unwrap_or_else(|| "linux".to_string())
    } else {
        "unknown".to_string()
    };

    Ok(base_os)
}

#[tauri::command]
async fn update_window_menu_tabs(app: AppHandle, tabs: Vec<TabItem>) -> Result<(), String> {
    let new_menu = menu::menu(&app, &tabs).map_err(|e| e.to_string())?;
    let _ = app.set_menu(new_menu);

    Ok(())
}

fn backup_databases(app: &App) -> tauri::Result<()> {
    let version = app.package_info().version.to_string();
    // This seems like the wrong directory to use, but it's what the SQL plugin uses so ¯\_(ツ)_/¯
    let base_dir = app.path().app_config_dir()?;
    let backup_dir = base_dir.join("backup");
    let target_dir = backup_dir.join(version);

    // On first start, the main dir may not exist
    if !fs::exists(&base_dir)? {
        fs::create_dir(&base_dir)?
    }

    if !fs::exists(&backup_dir)? {
        fs::create_dir(&backup_dir)?
    }

    if !fs::exists(&target_dir)? {
        fs::create_dir(&target_dir)?;

        for entry in fs::read_dir(&base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let target = target_dir.join(path.file_name().unwrap());
                fs::copy(&path, &target)?;
            }
        }
    }

    Ok(())
}

fn show_window(app: &AppHandle) {
    let windows = app.webview_windows();

    windows
        .values()
        .next()
        .expect("Sorry, no window found")
        .set_focus()
        .expect("Can't Bring Window to Focus");
}

async fn apply_runbooks_migrations(app: &AppHandle) -> eyre::Result<()> {
    let state = app.state::<crate::state::AtuinState>();
    let pool = state.db_instances.get_pool("runbooks").await?;
    sqlx::migrate!("./migrations/runbooks").run(&pool).await?;

    Ok(())
}

fn main() {
    let dev_prefix = if tauri::is_dev() {
        Some(env::var("DEV_PREFIX").unwrap_or("dev".to_string()))
    } else {
        None
    };

    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::BrightWhite)
        .debug(Color::Blue)
        .trace(Color::BrightBlack);

    let builder = tauri::Builder::default().plugin(
        tauri_plugin_log::Builder::new()
            .targets([
                tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stderr),
                tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                    file_name: None,
                }),
            ])
            .level(
                std::env::var("RUST_LOG")
                    .ok()
                    .and_then(|level| level.parse().ok())
                    .unwrap_or(log::LevelFilter::Info),
            )
            .level_for(
                "atuin_desktop",
                std::env::var("ATUIN_LOG")
                    .or_else(|_| std::env::var("RUST_LOG"))
                    .ok()
                    .and_then(|level| level.parse().ok())
                    .unwrap_or(log::LevelFilter::Info),
            )
            .level_for(
                "atuin_desktop_runtime",
                std::env::var("ATUIN_LOG")
                    .or_else(|_| std::env::var("RUST_LOG"))
                    .ok()
                    .and_then(|level| level.parse().ok())
                    .unwrap_or(log::LevelFilter::Info),
            )
            .max_file_size(20_000_000)
            .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(4))
            .with_colors(tauri_plugin_log::fern::colors::ColoredLevelConfig::default())
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "{color_reset}[{date}][{level}]{target}{color_line} {message}{color_reset}",
                    color_reset = format_args!("\x1B[0m"),
                    color_line = format_args!(
                        "\x1B[{}m",
                        colors_line.get_color(&record.level()).to_fg_str()
                    ),
                    date = humantime::format_rfc3339(std::time::SystemTime::now()),
                    target =
                        format_args!("\x1B[{}m[{}]", Color::White.to_fg_str(), record.target()),
                    level = colors_line.color(record.level()),
                    message = message
                ));
            })
            .build(),
    );
    let builder = if cfg!(debug_assertions) {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_window(app);
            println!("app opened with {argv:?}");
        }))
    };

    let app = builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            list,
            search,
            prefix_search,
            global_stats,
            aliases,
            home_info,
            config,
            session,
            login,
            logout,
            register,
            history_calendar,
            cli_settings,
            get_app_version,
            get_platform_info,
            update_window_menu_tabs,
            advanced_settings::get_advanced_settings,
            run::pty::pty_write,
            run::pty::pty_resize,
            run::shell::check_binary_exists,
            install::install_cli,
            install::is_cli_installed,
            install::setup_cli,
            install::get_default_shell,
            blocks::postgres::command::postgres_query,
            blocks::postgres::command::postgres_execute,
            dotfiles::aliases::import_aliases,
            dotfiles::aliases::delete_alias,
            dotfiles::aliases::set_alias,
            dotfiles::vars::vars,
            dotfiles::vars::delete_var,
            dotfiles::vars::set_var,
            secret::save_password,
            secret::load_password,
            secret::delete_password,
            runbooks::ydoc::save_ydoc_for_runbook,
            runbooks::ydoc::load_ydoc_for_runbook,
            runbooks::runbook::export_atrb,
            runbooks::runbook::delete_runbook_cleanup,
            file::find_files,
            font::list_fonts,
            main_window::save_window_info,
            main_window::show_window,
            commands::exec_log::log_execution,
            commands::dependency::can_run,
            commands::pty_store::runbook_kill_all_ptys,
            commands::workflow::serial::workflow_serial,
            commands::workflow::serial::workflow_block_start_event,
            commands::workflow::serial::workflow_stop,
            commands::stats::command_stats,
            commands::template::set_template_var,
            commands::template::get_template_var,
            commands::feedback::send_feedback,
            commands::blocks::execute_block,
            commands::blocks::cancel_block_execution,
            commands::blocks::open_document,
            commands::blocks::update_document,
            commands::blocks::get_flattened_block_context,
            commands::blocks::get_block_state,
            commands::blocks::notify_block_kv_value_changed,
            commands::blocks::reset_runbook_state,
            commands::blocks::respond_to_block_prompt,
            commands::blocks::remove_stored_context_for_document,
            commands::blocks::start_serial_execution,
            commands::blocks::stop_serial_execution,
            commands::blocks::get_runbook_content,
            commands::events::subscribe_to_events,
            commands::updates::check_for_updates,
            commands::workspaces::copy_welcome_workspace,
            commands::workspaces::reset_workspaces,
            commands::workspaces::watch_workspace,
            commands::workspaces::unwatch_workspace,
            commands::workspaces::create_workspace,
            commands::workspaces::rename_workspace,
            commands::workspaces::read_dir,
            commands::workspaces::get_workspace_id_by_folder,
            commands::workspaces::create_runbook,
            commands::workspaces::delete_runbook,
            commands::workspaces::save_runbook,
            commands::workspaces::get_runbook,
            commands::workspaces::create_folder,
            commands::workspaces::rename_folder,
            commands::workspaces::delete_folder,
            commands::workspaces::move_items,
            commands::workspaces::move_items_between_workspaces,
            commands::audio::list_sounds,
            commands::audio::play_sound,
            shared_state::get_shared_state_document,
            shared_state::push_optimistic_update,
            shared_state::update_shared_state_document,
            shared_state::delete_shared_state_document,
            shared_state::remove_optimistic_updates,
            shellcheck::shellcheck,
        ])
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_http::init())
        .register_uri_scheme_protocol("resources", |ctx, request| {
            let filename = request.uri().path()[1..].to_string();
            let filename = if filename.is_empty() {
                request
                    .uri()
                    .host()
                    .expect("Resource URI must have a host")
                    .to_string()
            } else {
                format!(
                    "{}/{}",
                    request.uri().host().expect("Resource URI must have a host"),
                    filename
                )
            };

            let resources_dir = ctx
                .app_handle()
                .path()
                .resolve("resources", BaseDirectory::Resource);

            if let Err(e) = resources_dir {
                log::error!("Failed to resolve resources directory: {}", e);
                return http::Response::builder()
                    .status(500)
                    .header(http::header::CONTENT_TYPE, "text/plain")
                    .body(b"Internal server error".to_vec())
                    .unwrap();
            }

            if let Ok(data) = fs::read(resources_dir.unwrap().join(&filename)) {
                http::Response::builder().body(data).unwrap()
            } else {
                http::Response::builder()
                    .status(404)
                    .header(http::header::CONTENT_TYPE, "text/plain")
                    .body(b"Not found".to_vec())
                    .unwrap()
            }
        })
        .setup(|app| {
            let advanced_settings_path = app
                .path()
                .app_config_dir()
                .expect("Failed to get app config dir")
                .join("advanced_settings");

            let advanced_settings = AdvancedSettings::load(&advanced_settings_path)?;
            app.manage(advanced_settings.clone());

            // Load login shell environment early in startup
            // This is best-effort only and should never crash the app
            #[cfg(unix)]
            if advanced_settings.copy_shell_env {
                run_async_command(async {
                    match crate::util::load_login_shell_environment().await {
                        Ok(()) => {
                            log::info!("Successfully loaded login shell environment");
                        }
                        Err(e) => {
                            log::warn!("Failed to load login shell environment: {}", e);
                        }
                    }
                });
            } else {
                log::warn!(
                    "Skipping login shell environment copy due to advanced configuration settings"
                );
            }

            backup_databases(app)?;

            let handle = app.handle();
            menu::initialize_menu_handlers(handle);

            let app_path = app
                .path()
                .app_config_dir()
                .expect("Failed to get app config dir");

            let use_hub_updater_service = env::var("USE_HUB_UPDATER_SERVICE").is_ok();
            if use_hub_updater_service {
                log::info!("Using Hub updater service");
            }

            let handle_clone = handle.clone();
            run_async_command(async move {
                handle.manage(state::AtuinState::new(
                    dev_prefix,
                    app_path,
                    use_hub_updater_service,
                ));
                handle
                    .state::<state::AtuinState>()
                    .init(&handle_clone)
                    .await
                    .expect("Failed to initialize application state");
            });

            let handle_clone = handle.clone();
            run_async_command(async move {
                apply_runbooks_migrations(&handle_clone).await.unwrap();
            });

            handle.set_menu(menu::menu(handle, &[]).expect("Failed to build menu"))?;

            let handle_clone = handle.clone();
            tauri::async_runtime::spawn(async move {
                main_window::create_main_window(&handle_clone)
                    .await
                    .unwrap();
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(move |handle, event| {
        if let RunEvent::Exit = event {
            run_async_command(async move {
                let state = handle.state::<state::AtuinState>();
                state.shutdown().await.unwrap();
            });
        }
    })
}

/// Allows blocking on async code without creating a nested runtime.
pub fn run_async_command<F: std::future::Future>(cmd: F) -> F::Output {
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => tokio::task::block_in_place(|| runtime.block_on(cmd)),
        _ => tauri::async_runtime::block_on(cmd),
    }
}
