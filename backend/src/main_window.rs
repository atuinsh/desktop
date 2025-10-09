use crate::{kv, state};
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::WebviewWindowBuilder;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
};

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub(crate) struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl WindowState {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn default(size: PhysicalSize<u32>) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }
}

fn get_os() -> String {
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        "unknown".to_string()
    }
}

pub(crate) async fn create_main_window(app: &AppHandle) -> Result<(), String> {
    let dev_prefix = app.state::<state::AtuinState>().dev_prefix.clone();
    let channel = env!("APP_CHANNEL");

    // To calculate the physical size of the window, we need to know the scale factor of the monitor.
    // To determine which monitor the window is on, we need to know the position of the window.
    let saved_window_state = load_window_state(app).await.unwrap_or(None);
    let position = saved_window_state
        .as_ref()
        .map(|state| (state.x, state.y))
        .unwrap_or((0, 0));
    let monitor = app
        .monitor_from_point(position.0 as f64, position.1 as f64)
        .unwrap_or(None);
    let scale_factor = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let monitor_size_phys = monitor
        .as_ref()
        .map(|m| *m.size())
        .unwrap_or(PhysicalSize::new(1200, 1000));

    let default_window_size_log = LogicalSize::new(1200, 1000);
    let mut default_window_size_phys: PhysicalSize<u32> =
        default_window_size_log.to_physical(scale_factor);
    default_window_size_phys.width = default_window_size_phys
        .width
        .min(monitor_size_phys.width as u32);
    default_window_size_phys.height = default_window_size_phys
        .height
        .min(monitor_size_phys.height as u32);

    let window_state = saved_window_state.unwrap_or(WindowState::default(default_window_size_phys));

    let mut query_elems: Vec<(&str, String)> = vec![
        ("os", get_os()),
        ("devPrefix", dev_prefix.clone().unwrap_or("dev".to_string())),
        ("channel", channel.to_string()),
    ];

    if std::env::var("NO_SYNC").is_ok() {
        query_elems.push(("noSync", "true".to_string()));
    }

    let query_string = query_elems
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<String>>()
        .join("&");

    let app_url = WebviewUrl::App(format!("index.html?{query_string}").into());

    let title = if dev_prefix.is_some() {
        format!("Atuin - {}", dev_prefix.unwrap())
    } else {
        "Atuin".to_string()
    };

    let title = if channel != "stable" {
        format!("{title} ({channel})")
    } else {
        title
    };

    let mut builder = WebviewWindowBuilder::new(app, "main", app_url)
        .title(title)
        .resizable(true)
        .fullscreen(false)
        .disable_drag_drop_handler()
        .min_inner_size(500.0, 500.0)
        .background_throttling(BackgroundThrottlingPolicy::Suspend)
        .visible(false);

    builder = {
        #[cfg(target_os = "macos")]
        {
            builder
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
        }
        #[cfg(not(target_os = "macos"))]
        {
            builder
        }
    };

    let window = builder.build().unwrap();
    if std::env::var("DEVTOOLS").is_ok() {
        window.open_devtools();
    }

    // For some reason, setting the window positions directly results in different
    // results than passing the values into the builder options.
    window
        .set_size(PhysicalSize::new(window_state.width, window_state.height))
        .map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(window_state.x, window_state.y))
        .map_err(|e| e.to_string())?;

    // Most of the time, the above is all that's needed. Sometimes however, the window
    // is positioned or sized incorrectly. This seems to happen most often with external
    // displays that have a different scale factor. This is a hacky fix, but it seems
    // to correct the issue within two attempts most of the time.
    let mut attempts = 0;
    while !is_correctly_sized_and_positioned(&window, &window_state) && attempts < 10 {
        attempts += 1;
        let target_size = PhysicalSize::new(window_state.width, window_state.height);
        let target_pos = PhysicalPosition::new(window_state.x, window_state.y);
        println!("target window state:  size: {target_size:?}, pos: {target_pos:?}");
        println!(
            "current window state: size: {:?}, pos: {:?}",
            window.outer_size().unwrap(),
            window.outer_position().unwrap()
        );
        println!("adjustment attempt: {attempts}");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        window
            .set_size(PhysicalSize::new(window_state.width, window_state.height))
            .map_err(|e| e.to_string())?;
        window
            .set_position(PhysicalPosition::new(window_state.x, window_state.y))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn is_correctly_sized_and_positioned(window: &WebviewWindow, window_state: &WindowState) -> bool {
    let position = window.outer_position().unwrap();
    let size = window.outer_size().unwrap();
    position.x == window_state.x
        && position.y == window_state.y
        && size.width == window_state.width
        && size.height == window_state.height
}

#[tauri::command]
pub(crate) async fn save_window_info(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").unwrap();
    let position = window.outer_position().unwrap();
    let size = window.outer_size().unwrap();

    let window_state = WindowState::new(position.x, position.y, size.width, size.height);
    save_window_state(&app, window_state).await?;

    Ok(())
}

#[tauri::command]
pub(crate) async fn show_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").unwrap();
    window.show().unwrap();
    window.set_focus().unwrap();
    Ok(())
}

async fn save_window_state(app: &AppHandle, state: WindowState) -> Result<(), String> {
    let db = kv::open_db(app).await.map_err(|e| e.to_string())?;
    kv::set(&db, "window_state_u32", &state).await
}

async fn load_window_state(app: &AppHandle) -> Result<Option<WindowState>, String> {
    let db = kv::open_db(app).await.map_err(|e| e.to_string())?;
    kv::get(&db, "window_state_u32").await
}
