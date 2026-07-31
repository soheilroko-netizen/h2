// main.rs - Tauri app entry with commands
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// ── Single-instance guard via named mutex (Windows) ──────────────────
// Prevents launching a second instance while one is already running.
#[cfg(target_os = "windows")]
fn check_single_instance() {
    use std::ffi::CString;
    use std::ptr;
    extern "system" {
        fn CreateMutexA(
            lpMutexAttributes: *mut std::ffi::c_void,
            bInitialOwner: i32,
            lpName: *const i8,
        ) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
    }
    let name = CString::new("Local\\stls-single-instance-mutex").unwrap();
    let handle = unsafe { CreateMutexA(ptr::null_mut(), 0, name.as_ptr()) };
    if handle.is_null() {
        eprintln!("[stls] CreateMutexA failed");
        return;
    }
    const ERROR_ALREADY_EXISTS: u32 = 183;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        println!("[stls] Another instance is already running — exiting.");
        std::process::exit(0);
    }
}

#[cfg(not(target_os = "windows"))]
fn check_single_instance() {}

use std::sync::Mutex;
use std::time::Instant;

// HTTP client with connection pooling for sing-box API
static SING_BOX_CLIENT: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();

fn sing_box_client() -> &'static reqwest::blocking::Client {
    SING_BOX_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .pool_max_idle_per_host(2)
            .build()
            .expect("reqwest client")
    })
}

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use config::{Config, ProfileStore};
use proxy::ProxyManager;

mod config;
mod proxy;
mod sysdns;

struct AppState {
    proxy: Mutex<ProxyManager>,
    started_at: Mutex<Option<Instant>>,
    prev_total: Mutex<(u64, u64)>,
    prev_time: Mutex<Option<Instant>>,
}

// ── Tray menu rebuild helper ───────────────────────────────────

fn update_tray_state(app: &tauri::AppHandle) {
    let state: tauri::State<AppState> = app.state::<AppState>();
    let running = state.proxy.lock().unwrap().is_running();
    drop(state);

    let profile_name = ProfileStore::load()
        .map(|s| s.active_profile)
        .unwrap_or_else(|_| "dakal-tls".to_string());

    let tooltip = if running {
        format!("dakal-tls VPN — {} (connected)", profile_name)
    } else {
        "dakal-tls VPN".to_string()
    };

    let show = MenuItemBuilder::with_id("show", "Show").build(app).unwrap();
    let hide = MenuItemBuilder::with_id("hide", "Hide").build(app).unwrap();
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app).unwrap();
    let profile = MenuItemBuilder::with_id("profile", &profile_name)
        .enabled(false)
        .build(app)
        .unwrap();

    let menu = if running {
        let disc = MenuItemBuilder::with_id("disconnect", "Disconnect")
            .build(app)
            .unwrap();
        MenuBuilder::new(app)
            .item(&profile)
            .item(&disc)
            .separator()
            .item(&show)
            .item(&hide)
            .separator()
            .item(&quit)
            .build()
            .unwrap()
    } else {
        let conn = MenuItemBuilder::with_id("connect", "Connect")
            .build(app)
            .unwrap();
        MenuBuilder::new(app)
            .item(&profile)
            .item(&conn)
            .separator()
            .item(&show)
            .item(&hide)
            .separator()
            .item(&quit)
            .build()
            .unwrap()
    };

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&tooltip));
        let _ = tray.set_menu(Some(menu));
    }
}

// ── Tauri commands ──────────────────────────────────────────────

#[tauri::command]
fn start_proxy(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.start().map_err(|e| e.to_string())?;
    *state.started_at.lock().unwrap() = Some(Instant::now());
    drop(proxy);
    update_tray_state(&app);
    Ok(result)
}

#[tauri::command]
fn stop_proxy(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.stop().map_err(|e| e.to_string())?;
    *state.started_at.lock().unwrap() = None;
    *state.prev_total.lock().unwrap() = (0, 0);
    *state.prev_time.lock().unwrap() = None;
    drop(proxy);
    update_tray_state(&app);
    Ok(result)
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Result<bool, String> {
    Ok(state.proxy.lock().unwrap().is_running())
}

#[tauri::command]
fn get_config() -> Result<Config, String> {
    let store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.get_active_config().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(config: Config) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store
        .update_active_config(config)
        .map_err(|e| e.to_string())?;
    Ok("Saved".into())
}

#[tauri::command]
fn get_profiles() -> Result<ProfileStore, String> {
    ProfileStore::load().map_err(|e| e.to_string())
}

#[tauri::command]
fn add_profile(name: String, config: Config) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.add_profile(name.clone(), config).map_err(|e| e.to_string())?;
    Ok(format!("Created '{}'", name))
}

#[tauri::command]
fn delete_profile(name: String) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.delete_profile(&name).map_err(|e| e.to_string())?;
    Ok(format!("Deleted '{}'", name))
}


#[tauri::command]
fn get_total_traffic() -> Result<String, String> {
    let client = sing_box_client();
    let resp = client
        .get("http://127.0.0.1:9097/connections")
        .header("Authorization", "Bearer dakal")
        .send()
        .map_err(|e| format!("request: {e}"))?;

    if !resp.status().is_success() {
        return Ok(r#"{"up":0,"down":0}"#.into());
    }

    let v: serde_json::Value = resp.json().map_err(|e| format!("json: {e}"))?;

    // sing-box /connections returns:
    //   { "download_total": N, "upload_total": N, "connections": [...] }
    // Older builds / forks only return the array -- sum per-conn bytes.
    let up = v["upload_total"].as_u64().unwrap_or(0);
    let down = v["download_total"].as_u64().unwrap_or(0);
    if up == 0 && down == 0 {
        let arr_opt = v["connections"].as_array().or_else(|| v.as_array());
        if let Some(arr) = arr_opt {
            let mut sum_up: u64 = 0;
            let mut sum_down: u64 = 0;
            for c in arr {
                sum_up = sum_up.saturating_add(c["upload"].as_u64().unwrap_or(0));
                sum_down = sum_down.saturating_add(c["download"].as_u64().unwrap_or(0));
            }
            return Ok(format!(r#"{{"up":{},"down":{}}}"#, sum_up, sum_down));
        }
    }
    Ok(format!(r#"{{"up":{},"down":{}}}"#, up, down))
}

#[tauri::command]
fn get_uptime(state: State<AppState>) -> Result<u64, String> {
    let guard = state.started_at.lock().unwrap();
    match *guard {
        Some(start) => Ok(start.elapsed().as_secs()),
        None => Ok(0),
    }
}

// Batched status for frontend polling - single invoke returns all metrics
#[tauri::command]
fn get_full_status(state: State<AppState>) -> Result<FullStatus, String> {
    let proxy = state.proxy.lock().unwrap();
    let running = proxy.is_running();
    drop(proxy);

    let profile_name = ProfileStore::load()
        .map(|s| s.active_profile)
        .unwrap_or_else(|_| "Default".to_string());

    if !running {
        return Ok(FullStatus {
            running: false,
            profile: profile_name,
            server: None,
            uptime_secs: 0,
            pid: None,
            traffic_up: 0,
            traffic_down: 0,
            total_up: 0,
            total_down: 0,
            log_lines: Vec::new(),
        });
    }

    let started_at = state.started_at.lock().unwrap();
    let uptime_secs = started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    drop(started_at);

    // Get server address from active config
    let server_addr = ProfileStore::load()
        .ok()
        .and_then(|s| s.get_active_config().ok())
        .map(|c| c.server_address.clone());

    // Get PID from proxy
    let proxy2 = state.proxy.lock().unwrap();
    let pid = proxy2.pid();
    let log_path = proxy2.debug_log_path.clone();
    drop(proxy2);

    // Read last 100 log lines
    let log_lines: Vec<String> = std::fs::read_to_string(&log_path)
        .map(|f| {
            f.lines()
                .rev()
                .take(100)
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        })
        .unwrap_or_default();

    let client = sing_box_client();

    // /traffic is SSE (not JSON) — only /connections gives totals
    let connections_resp = client
        .get("http://127.0.0.1:9097/connections")
        .header("Authorization", "Bearer dakal")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .ok()
        .and_then(|r| r.json::<serde_json::Value>().ok());

    let (cur_up, cur_down) = connections_resp
        .map(|v| {
            let up = v["upload_total"].as_u64().unwrap_or(0);
            let down = v["download_total"].as_u64().unwrap_or(0);
            if up == 0 && down == 0 {
                let arr_opt = v["connections"].as_array().or_else(|| v.as_array());
                if let Some(arr) = arr_opt {
                    let mut sum_up: u64 = 0;
                    let mut sum_down: u64 = 0;
                    for c in arr {
                        sum_up = sum_up.saturating_add(c["upload"].as_u64().unwrap_or(0));
                        sum_down = sum_down.saturating_add(c["download"].as_u64().unwrap_or(0));
                    }
                    (sum_up, sum_down)
                } else {
                    (0, 0)
                }
            } else {
                (up, down)
            }
        })
        .unwrap_or((0, 0));

    // Calculate traffic speed (bytes/sec) from delta
    let now = Instant::now();
    let mut prev_total = state.prev_total.lock().unwrap();
    let mut prev_time = state.prev_time.lock().unwrap();

    let (traffic_up, traffic_down) = if let Some(prev_t) = *prev_time {
        let elapsed = now.duration_since(prev_t).as_secs_f64();
        if elapsed > 0.5 {
            let up_delta = cur_up.saturating_sub(prev_total.0);
            let down_delta = cur_down.saturating_sub(prev_total.1);
            *prev_total = (cur_up, cur_down);
            *prev_time = Some(now);
            ((up_delta as f64 / elapsed) as u64, (down_delta as f64 / elapsed) as u64)
        } else {
            // Too soon, return previous rate (0 on first call)
            (0, 0)
        }
    } else {
        *prev_total = (cur_up, cur_down);
        *prev_time = Some(now);
        (0, 0)
    };
    drop(prev_total);
    drop(prev_time);

    Ok(FullStatus {
        running: true,
        profile: profile_name,
        server: server_addr,
        uptime_secs,
        pid,
        traffic_up,
        traffic_down,
        total_up: cur_up,
        total_down: cur_down,
        log_lines,
    })
}

#[derive(serde::Serialize)]
struct FullStatus {
    running: bool,
    profile: String,
    server: Option<String>,
    uptime_secs: u64,
    pid: Option<u32>,
    traffic_up: u64,
    traffic_down: u64,
    total_up: u64,
    total_down: u64,
    log_lines: Vec<String>,
}

#[tauri::command]
fn get_log(state: State<AppState>) -> Result<String, String> {
    let proxy = state.proxy.lock().unwrap();
    if let Some(f) = std::fs::read_to_string(&proxy.debug_log_path).ok() {
        Ok(f)
    } else {
        Ok("No log available".to_string())
    }
}

#[tauri::command]
fn real_ping(state: State<AppState>) -> Result<String, String> {
    let running = state.proxy.lock().unwrap().is_running();
    if !running {
        return Err("VPN not connected".into());
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let target = "http://www.gstatic.com/generate_204";

    if let Err(e) = client.get(target).send() {
        return Err(format!("warmup failed: {}", e));
    }

    let start = Instant::now();
    let resp = client.get(target).send().map_err(|e| format!("measure failed: {}", e))?;
    if !resp.status().is_success() && resp.status().as_u16() != 204 {
        return Err(format!("bad status: {}", resp.status()));
    }
    let elapsed = start.elapsed();

    let ms = (elapsed.as_micros() as f64 / 1000.0) as u64;
    Ok(format!("{}ms", ms))
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().ok();
        win.set_focus().ok();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings.html".into()))
        .title("dakal-tls — Settings")
        .inner_size(580.0, 520.0)
        .resizable(true)
        .build()
        .map_err(|e| format!("settings window: {e}"))?;
    Ok(())
}

#[tauri::command]
fn switch_profile(name: String) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.switch_profile(&name).map_err(|e| e.to_string())?;
    Ok(format!("Switched to '{}'", name))
}

#[tauri::command]
fn switch_profile_stop(name: String, state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    if proxy.is_running() {
        proxy.stop().map_err(|e| e.to_string())?;
        *state.started_at.lock().unwrap() = None;
        *state.prev_total.lock().unwrap() = (0, 0);
        *state.prev_time.lock().unwrap() = None;
    }
    drop(proxy);

    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.switch_profile(&name).map_err(|e| e.to_string())?;
    Ok(format!("Switched to '{}'", name))
}

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("dakal-tls v5")
        .inner_size(500.0, 520.0)
        .resizable(true)
        .build()?;
    Ok(())
}

fn main() {
    check_single_instance();

    let panic_log = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("stls-panic.log");
    std::fs::write(&panic_log, "stls starting...\n").ok();
    let pl = panic_log.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("PANIC: {}\n", info);
        std::fs::write(&pl, &msg).ok();
    }));

    let proxy_manager = ProxyManager::new().expect("Failed to init proxy manager");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            proxy: Mutex::new(proxy_manager),
            started_at: Mutex::new(None),
            prev_total: Mutex::new((0, 0)),
            prev_time: Mutex::new(None),
        })
        .setup(|app| {
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let profile_name_startup = ProfileStore::load()
                .map(|s| s.active_profile)
                .unwrap_or_else(|_| "dakal-tls".to_string());
            let profile_item = MenuItemBuilder::with_id("profile", &profile_name_startup)
                .enabled(false)
                .build(app)?;
            let connect_item = MenuItemBuilder::with_id("connect", "Connect").build(app)?;
            let disconnect_item = MenuItemBuilder::with_id("disconnect", "Disconnect")
                .enabled(false)
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&profile_item)
                .item(&connect_item)
                .item(&disconnect_item)
                .separator()
                .item(&show_item)
                .item(&hide_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .tooltip("dakal-tls VPN")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                window.show().ok();
                                window.set_focus().ok();
                            }
                        }
                        "hide" => {
                            if let Some(window) = app.get_webview_window("main") {
                                window.hide().ok();
                            }
                        }
                        "connect" => {
                            let state = app.state::<AppState>();
                            let mut proxy = state.proxy.lock().unwrap();
                            if !proxy.is_running() {
                                let _ = proxy.start();
                                *state.started_at.lock().unwrap() = Some(Instant::now());
                            }
                            drop(proxy);
                            update_tray_state(app);
                        }
                        "disconnect" => {
                            let state = app.state::<AppState>();
                            let mut proxy = state.proxy.lock().unwrap();
                            if proxy.is_running() {
                                let _ = proxy.stop();
                                *state.started_at.lock().unwrap() = None;
                            }
                            drop(proxy);
                            update_tray_state(app);
                        }
                        "quit" => {
                            let state = app.state::<AppState>();
                            let mut proxy = state.proxy.lock().unwrap();
                            if proxy.is_running() {
                                let _ = proxy.stop();
                            }
                            drop(proxy);
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().ok().unwrap_or(false) {
                                window.hide().ok();
                            } else {
                                window.show().ok();
                                window.set_focus().ok();
                            }
                        }
                    }
                })
                .build(app)?;

            // Initial tray state (profile name + correct menu)
            update_tray_state(&app.handle());

            create_main_window(&app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    window.hide().ok();
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            start_proxy,
            stop_proxy,
            get_config,
            save_config,
            get_profiles,
            add_profile,
            delete_profile,
            switch_profile,
            switch_profile_stop,
            real_ping,
            get_uptime,
            get_full_status,
            get_log,
            open_settings_window,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
