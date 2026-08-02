// main.rs - Tauri app entry with commands
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// ── Single-instance guard via named mutex (Windows) ──────────────────
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

mod config;
mod proxy;
mod sysdns;

use config::Config;
use proxy::ProxyManager;

struct AppState {
    proxy: Mutex<ProxyManager>,
    started_at: Mutex<Option<Instant>>,
    prev_total: Mutex<(u64, u64)>,
    prev_time: Mutex<Option<Instant>>,
    http_client: reqwest::blocking::Client,
    cached_log: Mutex<(std::time::SystemTime, Vec<String>)>,
}

// ── Tray menu rebuild helper ───────────────────────────────────

fn update_tray_state(app: &tauri::AppHandle) {
    let state: tauri::State<AppState> = app.state::<AppState>();
    let running = state.proxy.lock().unwrap().is_running();
    drop(state);

    let mode = config::load_mode();

    let tooltip = if running {
        format!("dakal-tls VPN — {} (connected)", mode)
    } else {
        "dakal-tls VPN".to_string()
    };

    let show = MenuItemBuilder::with_id("show", "Show").build(app).unwrap();
    let hide = MenuItemBuilder::with_id("hide", "Hide").build(app).unwrap();
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app).unwrap();
    let mode_item = MenuItemBuilder::with_id("mode", &mode)
        .enabled(false)
        .build(app)
        .unwrap();

    let menu = if running {
        let disc = MenuItemBuilder::with_id("disconnect", "Disconnect")
            .build(app)
            .unwrap();
        MenuBuilder::new(app)
            .item(&mode_item)
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
            .item(&mode_item)
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

fn stop_proxy_inner(state: &State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.stop().map_err(|e| e.to_string())?;
    *state.started_at.lock().unwrap() = None;
    *state.prev_total.lock().unwrap() = (0, 0);
    *state.prev_time.lock().unwrap() = None;
    Ok(result)
}

#[tauri::command]
fn stop_proxy(app: tauri::AppHandle, state: State<AppState>) -> Result<String, String> {
    let result = stop_proxy_inner(&state)?;
    update_tray_state(&app);
    Ok(result)
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Result<bool, String> {
    Ok(state.proxy.lock().unwrap().is_running())
}

#[tauri::command]
fn get_config() -> Result<Config, String> {
    Ok(config::get_active_config())
}

#[tauri::command]
fn set_mode(mode: String, state: State<AppState>) -> Result<String, String> {
    // Stop proxy if running (mode change requires restart)
    if state.proxy.lock().unwrap().is_running() {
        let _ = stop_proxy_inner(&state);
    }
    config::save_mode(&mode).map_err(|e| e.to_string())?;
    Ok(format!("Mode set to '{}'", mode))
}

#[tauri::command]
fn get_mode() -> Result<String, String> {
    Ok(config::load_mode())
}

#[tauri::command]
fn get_uptime(state: State<AppState>) -> Result<u64, String> {
    let guard = state.started_at.lock().unwrap();
    match *guard {
        Some(start) => Ok(start.elapsed().as_secs()),
        None => Ok(0),
    }
}

#[tauri::command]
fn get_full_status(state: State<AppState>) -> Result<FullStatus, String> {
    let proxy = state.proxy.lock().unwrap();
    let running = proxy.is_running();
    let pid = proxy.pid();
    let log_path = proxy.debug_log_path.clone();
    drop(proxy);

    let mode = config::load_mode();

    if !running {
        return Ok(FullStatus {
            running: false, mode, server: None, uptime_secs: 0, pid: None,
            traffic_up: 0, traffic_down: 0, total_up: 0, total_down: 0,
            log_lines: Vec::new(),
        });
    }

    let cfg = config::get_active_config();
    let uptime_secs = state.started_at.lock().unwrap().map(|s| s.elapsed().as_secs()).unwrap_or(0);

    // Read last 100 log lines (cache: only re-read if file changed)
    let log_lines = {
        let modified = std::fs::metadata(&log_path)
            .and_then(|m| m.modified())
            .ok();
        let mut cache = state.cached_log.lock().unwrap();
        let needs_refresh = match modified {
            Some(m) => cache.1.is_empty() || m > cache.0,
            None => false,
        };
        if needs_refresh {
            if let Ok(content) = std::fs::read_to_string(&log_path) {
                let mut lines: Vec<String> = content.lines().rev().take(100).map(String::from).collect();
                lines.reverse();
                cache.0 = std::time::SystemTime::now();
                cache.1 = lines;
            }
        }
        cache.1.clone()
    };

    // Get traffic from clash API
    let client = sing_box_client();
    let (cur_up, cur_down) = client
        .get("http://127.0.0.1:9097/connections")
        .header("Authorization", "Bearer dakal")
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .ok()
        .and_then(|r| r.json::<serde_json::Value>().ok())
        .map(|v| {
            let up = v["upload_total"].as_u64().unwrap_or(0);
            let down = v["download_total"].as_u64().unwrap_or(0);
            if up == 0 && down == 0 {
                // Fallback: sum from connections array
                v["connections"].as_array().map(|arr| {
                    arr.iter().fold((0u64, 0u64), |(u, d), c| (
                        u.saturating_add(c["upload"].as_u64().unwrap_or(0)),
                        d.saturating_add(c["download"].as_u64().unwrap_or(0)),
                    ))
                }).unwrap_or((0, 0))
            } else { (up, down) }
        })
        .unwrap_or((0, 0));

    // Calculate traffic speed (bytes/sec)
    let now = Instant::now();
    let mut prev_total = state.prev_total.lock().unwrap();
    let mut prev_time = state.prev_time.lock().unwrap();
    let (traffic_up, traffic_down) = prev_time.and_then(|prev_t| {
        let elapsed = now.duration_since(prev_t).as_secs_f64();
        if elapsed > 0.5 {
            let up_delta = cur_up.saturating_sub(prev_total.0);
            let down_delta = cur_down.saturating_sub(prev_total.1);
            *prev_total = (cur_up, cur_down);
            *prev_time = Some(now);
            Some(((up_delta as f64 / elapsed) as u64, (down_delta as f64 / elapsed) as u64))
        } else { None }
    }).unwrap_or_else(|| {
        *prev_total = (cur_up, cur_down);
        *prev_time = Some(now);
        (0, 0)
    });

    Ok(FullStatus {
        running: true, mode, server: Some(cfg.server_address), uptime_secs, pid,
        traffic_up, traffic_down, total_up: cur_up, total_down: cur_down, log_lines,
    })
}

#[derive(serde::Serialize)]
struct FullStatus {
    running: bool,
    mode: String,
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

    let target = "http://www.gstatic.com/generate_204";
    if let Err(e) = state.http_client.get(target).send() {
        return Err(format!("warmup failed: {}", e));
    }

    let start = Instant::now();
    let resp = state.http_client.get(target).send().map_err(|e| format!("measure failed: {}", e))?;
    if !resp.status().is_success() && resp.status().as_u16() != 204 {
        return Err(format!("bad status: {}", resp.status()));
    }

    let ms = (start.elapsed().as_micros() as f64 / 1000.0) as u64;
    Ok(format!("{}ms", ms))
}

#[tauri::command]
fn apply_h2_preset(name: String) -> Result<serde_json::Value, String> {
    let (up, down) = match name.as_str() {
        "adsl" => (4, 16),
        "4g" => (15, 30),
        "5g" => (40, 80),
        "max" => (80, 120),
        _ => return Err(format!("unknown preset: {}", name)),
    };
    config::save_h2_speeds(up, down).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "up_mbps": up, "down_mbps": down }))
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

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("dakal-tls v5")
        .inner_size(500.0, 624.0)
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
            http_client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap(),
            cached_log: Mutex::new((std::time::SystemTime::UNIX_EPOCH, Vec::new())),
        })
        .setup(|app| {
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let mode_startup = config::load_mode();
            let mode_item = MenuItemBuilder::with_id("mode", &mode_startup)
                .enabled(false)
                .build(app)?;
            let connect_item = MenuItemBuilder::with_id("connect", "Connect").build(app)?;
            let disconnect_item = MenuItemBuilder::with_id("disconnect", "Disconnect")
                .enabled(false)
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&mode_item)
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
            set_mode,
            get_mode,
            real_ping,
            get_uptime,
            get_full_status,
            get_log,
            open_settings_window,
            get_h2_speeds,
            apply_h2_preset,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
