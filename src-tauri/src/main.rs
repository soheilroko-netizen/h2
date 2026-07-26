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
    // Create a named mutex — if it already exists (ERROR_ALREADY_EXISTS),
    // another instance is running, so exit.
    let name = CString::new("Local\\stls-single-instance-mutex").unwrap();
    // SAFETY: trivially safe — passing a valid CString, checking for errors
    let handle = unsafe { CreateMutexA(ptr::null_mut(), 0, name.as_ptr()) };
    if handle.is_null() {
        eprintln!("[stls] CreateMutexA failed");
        return;
    }
    const ERROR_ALREADY_EXISTS: u32 = 183;
    // SAFETY: GetLastError is safe to call after CreateMutexA
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        println!("[stls] Another instance is already running — exiting.");
        std::process::exit(0);
    }
}

#[cfg(not(target_os = "windows"))]
fn check_single_instance() {}

use std::sync::Mutex;
use std::time::Instant;

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
}

// ── Tauri commands ──────────────────────────────────────────────

#[tauri::command]
fn start_proxy(state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.start().map_err(|e| e.to_string())?;
    *state.started_at.lock().unwrap() = Some(Instant::now());
    Ok(result)
}

#[tauri::command]
fn stop_proxy(state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.stop().map_err(|e| e.to_string())?;
    *state.started_at.lock().unwrap() = None;
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
    // Save to active profile, not standalone config.json
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
    }
    drop(proxy);

    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.switch_profile(&name).map_err(|e| e.to_string())?;
    Ok(format!("Switched to '{}'", name))
}

#[tauri::command]
fn get_traffic() -> Result<String, String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    // Connect to sing-box clash API /traffic SSE endpoint
    // Response: stream of lines like {"up":123,"down":456}
    let mut stream = TcpStream::connect_timeout(
        &"127.0.0.1:9097".parse().map_err(|e| format!("addr: {}", e))?,
        Duration::from_secs(2),
    )
    .map_err(|e| format!("connect: {}", e))?;

    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .map_err(|e| format!("set_read_timeout: {}", e))?;

    let req = "GET /traffic?token=shado HTTP/1.1\r\nHost: 127.0.0.1:9097\r\nConnection: close\r\n\r\n";
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write: {}", e))?;

    // Read stream — collect everything for up to 1.5s, keep last valid JSON line
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut all_data = String::new();
    let mut buf = [0u8; 2048];

    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                all_data.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(format!("read: {}", e)),
        }
    }

    // Find last line that looks like {"up":N,"down":N}
    let mut last_json: Option<(u64, u64)> = None;
    for line in all_data.lines() {
        let line = line.trim();
        if line.starts_with('{') && line.contains("\"up\"") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                let up = v["up"].as_u64().unwrap_or(0);
                let down = v["down"].as_u64().unwrap_or(0);
                last_json = Some((up, down));
            }
        }
    }

    let (up, down) = last_json.unwrap_or((0, 0));
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

#[tauri::command]
fn real_ping(state: State<AppState>) -> Result<String, String> {
    let running = state.proxy.lock().unwrap().is_running();
    if !running {
        return Err("VPN not connected".into());
    }

    // Two-request ping:
    // 1st request warms up TLS handshake
    // 2nd request measures established-connection latency only
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let target = "http://www.gstatic.com/generate_204";

    // Warmup request (discards result)
    if let Err(e) = client.get(target).send() {
        return Err(format!("warmup failed: {}", e));
    }

    // Measure request (actual latency)
    let start = Instant::now();
    let resp = client.get(target).send().map_err(|e| format!("measure failed: {}", e))?;
    if !resp.status().is_success() && resp.status().as_u16() != 204 {
        return Err(format!("bad status: {}", resp.status()));
    }
    let elapsed = start.elapsed();

    // Return single ms value, like v2rayN
    let ms = (elapsed.as_micros() as f64 / 1000.0) as u64;
    Ok(format!("{}ms", ms))
}

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("shado v5")
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
        })
        .setup(|app| {
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .item(&hide_item)
                .separator()
                .item(&quit_item)
                .build()?;

            TrayIconBuilder::new()
                .tooltip("shado VPN")
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
                        "quit" => {
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
            get_traffic,
            get_uptime,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}