// config.rs - 4 hardcoded profiles (no management UI)
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
    #[serde(default)]
    pub split_mode: String, // "full", "iran", "custom"
    #[serde(default)]
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

/// Load active profile name from config.json (default "netherlands-1-stls")
pub fn load_profile() -> String {
    match config_path() {
        Ok(path) if path.exists() => {
            let content = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v["profile"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "netherlands-1-stls".to_string())
        }
        _ => "netherlands-1-stls".to_string(),
    }
}

/// Save profile name to config.json
pub fn save_profile(profile: &str) -> Result<()> {
    let path = config_path()?;
    let mut existing = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    existing["profile"] = serde_json::Value::String(profile.to_string());
    fs::write(&path, serde_json::to_string_pretty(&existing)?)?;
    Ok(())
}

/// Save Hysteria2 speed test results to config.json
pub fn save_h2_speeds(up_mbps: u32, down_mbps: u32) -> Result<()> {
    let path = config_path()?;
    let mut existing = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    existing["h2_up_mbps"] = serde_json::json!(up_mbps);
    existing["h2_down_mbps"] = serde_json::json!(down_mbps);
    fs::write(&path, serde_json::to_string_pretty(&existing)?)?;
    Ok(())
}

/// Load Hysteria2 speed test results from config.json
pub fn load_h2_speeds() -> (u32, u32) {
    match config_path() {
        Ok(path) if path.exists() => {
            let content = fs::read_to_string(&path).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            let up = v["h2_up_mbps"].as_u64().unwrap_or(h2_mbps_up_default() as u64) as u32;
            let down = v["h2_down_mbps"].as_u64().unwrap_or(h2_mbps_down_default() as u64) as u32;
            (up, down)
        }
        _ => (h2_mbps_up_default(), h2_mbps_down_default()),
    }
}

/// Default Hysteria2 upload bandwidth in MBps
pub fn h2_mbps_up_default() -> u32 { 40 }

/// Default Hysteria2 download bandwidth in MBps
pub fn h2_mbps_down_default() -> u32 { 80 }

/// Save split tunnel settings to config.json
pub fn save_split_settings(split_mode: &str, split_rules: Vec<SplitRule>) -> Result<()> {
    let path = config_path()?;
    let mut existing = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    existing["split_mode"] = serde_json::Value::String(split_mode.to_string());
    existing["split_rules"] = serde_json::to_value(split_rules)?;
    fs::write(&path, serde_json::to_string_pretty(&existing)?)?;
    Ok(())
}

/// Load split tunnel settings from config.json
pub fn load_split_settings() -> (String, Vec<SplitRule>) {
    match config_path() {
        Ok(path) if path.exists() => {
            let content = fs::read_to_string(&path).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            let mode = v["split_mode"].as_str().unwrap_or("full").to_string();
            let rules = v["split_rules"].as_array().map(|arr| {
                arr.iter().filter_map(|r| {
                    Some(SplitRule {
                        pattern: r["pattern"].as_str()?.to_string(),
                        process_names: r["process_names"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                        folder_paths: r["folder_paths"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                    })
                }).collect()
            }).unwrap_or_default();
            (mode, rules)
        }
        _ => ("full".to_string(), vec![]),
    }
}

/// Get config for a specific profile
pub fn get_profile_config(profile: &str) -> Config {
    let (up, down) = load_h2_speeds();
    
    match profile {
        "netherlands-1-stls" => Config {
            server_address: "ns.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "tE+3/qlN/orCZRVUutWouysZ8BQs4RWzq46WK6CDGG4=".to_string(),
            stls_port: 8553,
            stls_password: "y2lachetore".to_string(),
            stls_sni: "dl.google.com".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "shadowtls".to_string(),
            h2_port: 40001,
            h2_password: "".to_string(),
            h2_sni: "".to_string(),
            h2_insecure: false,
            h2_obfs: "".to_string(),
            h2_obfs_password: "".to_string(),
            h2_mport: "".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "netherlands-1-h2" => Config {
            server_address: "ns.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "".to_string(),
            stls_port: 8553,
            stls_password: "".to_string(),
            stls_sni: "".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "hysteria2".to_string(),
            h2_port: 40001,
            h2_password: "testpass1".to_string(),
            h2_sni: "ns.baft.uk".to_string(),
            h2_insecure: false,
            h2_obfs: "salamander".to_string(),
            h2_obfs_password: "testobfspass".to_string(),
            h2_mport: "40001-45000".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "germany-1-stls" => Config {
            server_address: "ns.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "tE+3/qlN/orCZRVUutWouysZ8BQs4RWzq46WK6CDGG4=".to_string(),
            stls_port: 8553,
            stls_password: "y2lachetore".to_string(),
            stls_sni: "dl.google.com".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "shadowtls".to_string(),
            h2_port: 40001,
            h2_password: "".to_string(),
            h2_sni: "".to_string(),
            h2_insecure: false,
            h2_obfs: "".to_string(),
            h2_obfs_password: "".to_string(),
            h2_mport: "".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "germany-1-h2" => Config {
            server_address: "ns.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "".to_string(),
            stls_port: 8553,
            stls_password: "".to_string(),
            stls_sni: "".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "hysteria2".to_string(),
            h2_port: 40001,
            h2_password: "testpass1".to_string(),
            h2_sni: "ns.baft.uk".to_string(),
            h2_insecure: false,
            h2_obfs: "salamander".to_string(),
            h2_obfs_password: "testobfspass".to_string(),
            h2_mport: "40001-45000".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "germany-3-stls" => Config {
            server_address: "de3.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "tE+3/qlN/orCZRVUutWouysZ8BQs4RWzq46WK6CDGG4=".to_string(),
            stls_port: 8553,
            stls_password: "y2lachetore".to_string(),
            stls_sni: "dl.google.com".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "shadowtls".to_string(),
            h2_port: 40001,
            h2_password: "".to_string(),
            h2_sni: "".to_string(),
            h2_insecure: false,
            h2_obfs: "".to_string(),
            h2_obfs_password: "".to_string(),
            h2_mport: "".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "germany-3-h2" => Config {
            server_address: "de3.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "".to_string(),
            stls_port: 8553,
            stls_password: "".to_string(),
            stls_sni: "".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "hysteria2".to_string(),
            h2_port: 40001,
            h2_password: "testpass1".to_string(),
            h2_sni: "de3.baft.uk".to_string(),
            h2_insecure: false,
            h2_obfs: "salamander".to_string(),
            h2_obfs_password: "testobfspass".to_string(),
            h2_mport: "40001-45000".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "finland-1-stls" => Config {
            server_address: "fn.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "tE+3/qlN/orCZRVUutWouysZ8BQs4RWzq46WK6CDGG4=".to_string(),
            stls_port: 8553,
            stls_password: "y2lachetore".to_string(),
            stls_sni: "dl.google.com".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "shadowtls".to_string(),
            h2_port: 40001,
            h2_password: "".to_string(),
            h2_sni: "".to_string(),
            h2_insecure: false,
            h2_obfs: "".to_string(),
            h2_obfs_password: "".to_string(),
            h2_mport: "".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        "finland-1-h2" => Config {
            server_address: "fn.baft.uk".to_string(),
            ss_port: 8380,
            ss_password: "".to_string(),
            stls_port: 8553,
            stls_password: "".to_string(),
            stls_sni: "".to_string(),
            socks5_port: 1080,
            mtu: None,
            split_mode: "full".to_string(),
            split_rules: vec![],
            mode: "hysteria2".to_string(),
            h2_port: 40001,
            h2_password: "testpass1".to_string(),
            h2_sni: "fn.baft.uk".to_string(),
            h2_insecure: false,
            h2_obfs: "salamander".to_string(),
            h2_obfs_password: "testobfspass".to_string(),
            h2_mport: "40001-45000".to_string(),
            h2_up_mbps: up,
            h2_down_mbps: down,
            h2_auto: false,
        },
        _ => get_profile_config("netherlands-1-stls"), // fallback
    }
}

/// Get config for active profile
pub fn get_active_config() -> Config {
    let profile = load_profile();
    let mut cfg = get_profile_config(&profile);
    
    // Load split tunnel settings from config.json (overrides profile defaults)
    let (split_mode, split_rules) = load_split_settings();
    cfg.split_mode = split_mode;
    cfg.split_rules = split_rules;
    
    cfg
}

/// Legacy: load mode from active profile
pub fn load_mode() -> String {
    let profile = load_profile();
    let cfg = get_profile_config(&profile);
    cfg.mode
}

/// Legacy: save mode (maps to profile switch)
pub fn save_mode(mode: &str) -> Result<()> {
    // When switching mode, keep current server
    let current = load_profile();
    let server = if current.starts_with("netherlands") { "netherlands" }
                 else if current.starts_with("germany") { "germany" }
                 else { "finland" };
    let suffix = if current.contains("germany-3") { "-3" } else { "-1" };
    let new_profile = match mode {
        "shadowtls" => format!("{}{}-stls", server, suffix),
        "hysteria2" => format!("{}{}-h2", server, suffix),
        _ => "netherlands-1-stls".to_string(),
    };
    save_profile(&new_profile)
}
