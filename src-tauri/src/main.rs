// main.rs - Tauri app entry with commands
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::net::ToSocketAddrs;
use tauri::{
    State,
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    WebviewUrl, WebviewWindowBuilder,
};

mod config;
mod proxy;
mod sysdns;

use config::Config;
use config::ProfileStore;
use proxy::ProxyManager;

struct AppState {
    proxy: Mutex<ProxyManager>,
}

#[tauri::command]
fn get_status(state: State<AppState>) -> Result<bool, String> {
    let proxy = state.proxy.lock().unwrap();
    Ok(proxy.is_running())
}

#[tauri::command]
fn start_proxy(state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    proxy.start().map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_proxy(state: State<AppState>) -> Result<String, String> {
    let mut proxy = state.proxy.lock().unwrap();
    proxy.stop().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_config() -> Result<Config, String> {
    Config::load().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_config(config: Config) -> Result<String, String> {
    config.save().map_err(|e| e.to_string())?;
    Ok("Configuration saved".to_string())
}

#[tauri::command]
fn get_profiles() -> Result<ProfileStore, String> {
    ProfileStore::load().map_err(|e| e.to_string())
}

#[tauri::command]
fn add_profile(name: String, config: Config) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store
        .add_profile(name.clone(), config)
        .map_err(|e| e.to_string())?;
    Ok(format!("Profile '{}' added", name))
}

#[tauri::command]
fn delete_profile(name: String) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store
        .delete_profile(&name)
        .map_err(|e| e.to_string())?;
    Ok(format!("Profile '{}' deleted", name))
}

#[tauri::command]
fn switch_profile(name: String) -> Result<String, String> {
    let mut store = ProfileStore::load().map_err(|e| e.to_string())?;
    store
        .switch_profile(&name)
        .map_err(|e| e.to_string())?;
    Ok(format!("Switched to profile '{}'", name))
}

#[tauri::command]
fn ping_server(address: String, port: u16) -> Result<String, String> {
    use std::net::TcpStream;
    use std::time::Instant;

    let addr = format!("{}:{}", address, port);
    let socket_addrs = addr
        .to_socket_addrs()
        .map_err(|e| format!("DNS fail: {}", e))?
        .collect::<Vec<_>>();

    if socket_addrs.is_empty() {
        return Err("No IP resolved".into());
    }

    let start = Instant::now();
    TcpStream::connect_timeout(&socket_addrs[0], std::time::Duration::from_secs(5))
        .map_err(|e| format!("Connect fail: {}", e))?;
    let us = start.elapsed().as_micros();
    if us < 1000 {
        Ok(format!("<1ms"))
    } else {
        Ok(format!("{:.1}ms", us as f64 / 1000.0))
    }
}

fn create_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("shado v5")
        .inner_size(500.0, 480.0)
        .resizable(true)
        .build()?;
    Ok(())
}

fn main() {
    // Log panics to file for debugging silent crashes
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
        })
        .setup(|app| {
            // Build tray menu
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
                    // Minimize to tray instead of closing
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
            ping_server,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
