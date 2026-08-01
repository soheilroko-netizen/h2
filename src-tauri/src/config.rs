// config.rs - App configuration management
// Simplified: two baked-in profiles (ShadowTLS, Hysteria2), no ProfileStore
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server_address: String,
    pub ss_port: u16,
    pub ss_password: String,
    pub stls_port: u16,
    pub stls_password: String,
    pub stls_sni: String,
    pub socks5_port: u16,
    pub mtu: Option<u32>,
    pub split_rules: Vec<SplitRule>,

    pub mode: String,

    // Hysteria2 fields
    pub h2_port: u16,
    pub h2_password: String,
    pub h2_sni: String,
    pub h2_insecure: bool,
    pub h2_obfs: String,
    pub h2_obfs_password: String,
    pub h2_mport: String,
    #[serde(default = "h2_mbps_up_default")]
    pub h2_up_mbps: u32,
    #[serde(default = "h2_mbps_down_default")]
    pub h2_down_mbps: u32,
    #[serde(default = "h2_auto_default")]
    pub h2_auto: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SplitRule {
    pub pattern: String,
    #[serde(default)]
    pub process_names: Vec<String>,
    #[serde(default)]
    pub folder_paths: Vec<String>,
}

fn config_path() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "stls", "stls")
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    let config_dir = proj_dirs.config_dir();
    fs::create_dir_all(config_dir)?;
    Ok(config_dir.join("config.json"))
}

/// Load the active mode from config.json (or default "shadowtls")
pub fn load_mode() -> String {
    match config_path() {
        Ok(path) if path.exists() => {
            let content = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v["mode"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "shadowtls".to_string())
        }
        _ => "shadowtls".to_string(),
    }
}

/// Save just the mode to config.json
pub fn save_mode(mode: &str) -> Result<()> {
    let path = config_path()?;
    let mut existing = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    existing["mode"] = serde_json::Value::String(mode.to_string());
    fs::write(&path, serde_json::to_string_pretty(&existing)?)?;
    Ok(())
}

/// Return the baked-in ShadowTLS default config
pub fn stls_defaults() -> Config {
    Config {
        server_address: "ns.baft.uk".to_string(),
        ss_port: 8380,
        ss_password: "tE+3/qlN/orCZRVUutWouysZ8BQs4RWzq46WK6CDGG4=".to_string(),
        stls_port: 8553,
        stls_password: "y2lachetore".to_string(),
        stls_sni: "dl.google.com".to_string(),
        socks5_port: 1080,
        mtu: None,
        split_rules: vec![],
        mode: "shadowtls".to_string(),

        h2_port: 40001,
        h2_password: "testpass1".to_string(),
        h2_sni: "ns.baft.uk".to_string(),
        h2_insecure: false,
        h2_obfs: "salamander".to_string(),
        h2_obfs_password: "testobfspass".to_string(),
        h2_mport: "40001-45000".to_string(),
        h2_up_mbps: h2_mbps_up_default(),
        h2_down_mbps: h2_mbps_down_default(),
        h2_auto: h2_auto_default(),
    }
}

/// Return the baked-in Hysteria2 default config
pub fn h2_defaults() -> Config {
    Config {
        server_address: "ns.baft.uk".to_string(),
        ss_port: 8380,
        ss_password: String::new(),
        stls_port: 8553,
        stls_password: String::new(),
        stls_sni: String::new(),
        socks5_port: 1080,
        mtu: None,
        split_rules: vec![],
        mode: "hysteria2".to_string(),

        // From: hysteria2://testuser1:testpass1@ns.baft.uk:40001?sni=ns.baft.uk&insecure=0&obfs=salamander&obfs-password=testobfspass&mport=40001-45000
        h2_port: 40001,
        h2_password: "testpass1".to_string(),
        h2_sni: "ns.baft.uk".to_string(),
        h2_insecure: false,
        h2_obfs: "salamander".to_string(),
        h2_obfs_password: "testobfspass".to_string(),
        h2_mport: "40001-45000".to_string(),
        h2_up_mbps: h2_mbps_up_default(),
        h2_down_mbps: h2_mbps_down_default(),
        h2_auto: h2_auto_default(),
    }
}

/// Default Hysteria2 upload bandwidth in MBps
pub fn h2_mbps_up_default() -> u32 { 8 }

/// Default Hysteria2 download bandwidth in MBps
pub fn h2_mbps_down_default() -> u32 { 24 }

/// Auto-tune flag default
pub fn h2_auto_default() -> bool { false }

/// Get the config for the active mode
pub fn get_active_config() -> Config {
    let mode = load_mode();
    match mode.as_str() {
        "hysteria2" => h2_defaults(),
        _ => stls_defaults(),
    }
}
