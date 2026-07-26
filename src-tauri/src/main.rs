use std::sync::Mutex;
use std::time::Instant;

use tauri::menu::{MenuBuilder, MenuItemBuilder, MenuItemKind};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewWindowBuilder};

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
    let result = proxy.start()?;
    *state.started_at.lock().unwrap() = Some(Instant::now());
    Ok(result)
}

#[tauri::command]
fn stop_proxy(state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    let result = proxy.stop()?;
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
    store.get_active_config()
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
    store.add_profile(&name, config).map_err(|e| e.to_string())?;
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
        proxy.stop()?;
        *state.started_at.lock().unwrap() = None;
    }
    drop(proxy);

    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store.switch_profile(&name).map_err(|e| e.to_string())?;
    Ok(format!("Switched to '{}'", name))
}

#[tauri::command]
fn get_traffic() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let resp = client
        .get("http://127.0.0.1:9097/connections?token=shado")
        .send()
        .map_err(|e| format!("connections req: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("bad status: {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().map_err(|e| format!("parse: {}", e))?;
    let up = body["uploadTotal"].as_u64().unwrap_or(0);
    let down = body["downloadTotal"].as_u64().unwrap_or(0);

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

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("http client: {}", e))?;

    let target = "http://www.gstatic.com/generate_204";
    let count = 5;
    let mut times = Vec::with_capacity(count as usize);

    for i in 0..count {
        let start = Instant::now();
        let resp = client
            .get(target)
            .send()
            .map_err(|e| format!("ping #{} failed: {}", i + 1, e))?;
        let elapsed = start.elapsed();
        let status = resp.status();
        if !status.is_success() && status.as_u16() != 204 {
            return Err(format!("bad status on ping #{}: {}", i + 1, status));
        }
        times.push(elapsed);
    }

    let us: Vec<u64> = times.iter().map(|t| t.as_micros() as u64).collect();
    let min = us.iter().min().copied().unwrap_or(0);
    let max = us.iter().max().copied().unwrap_or(0);
    let avg = us.iter().sum::<u64>() / us.len() as u64;

    let fmt = |us: u64| -> String {
        if us < 1000 {
            format!("<1ms")
        } else {
            format!("{:.0}ms", us as f64 / 1000.0)
        }
    };

    Ok(format!("avg {} | min {} | max {}", fmt(avg), fmt(min), fmt(max)))
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
            let show_item = MenuItemBuilder::with_id("show", "Show")
                .build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Hide")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;

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