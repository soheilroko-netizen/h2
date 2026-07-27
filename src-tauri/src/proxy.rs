// proxy.rs - sing-box proxy manager (VPN-only)
use anyhow::{bail, Context, Result};
use crate::config::Config;
use crate::sysdns;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs::{self};
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

// ── Windows helper: spawn without console window ──────────────────
#[cfg(target_os = "windows")]
fn no_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}
#[cfg(not(target_os = "windows"))]
fn no_window(cmd: &mut Command) -> &mut Command {
    cmd
}

// ── sing-box config structures ─────────────────────────────────────

#[derive(Serialize)]
struct SbConfig {
    log: SbLog,
    #[serde(skip_serializing_if = "Option::is_none")]
    experimental: Option<SbExperimental>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dns: Option<SbDns>,
    inbounds: Vec<SbInbound>,
    outbounds: Vec<SbOutbound>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<SbRoute>,
}

#[derive(Serialize)]
struct SbExperimental {
    #[serde(skip_serializing_if = "Option::is_none")]
    clash_api: Option<SbClashApi>,
}

#[derive(Serialize)]
struct SbClashApi {
    external_controller: String,
    secret: String,
    default_mode: String,
}

#[derive(Serialize)]
struct SbLog {
    disabled: bool,
    level: String,
    timestamp: bool,
}

#[derive(Serialize)]
struct SbDns {
    servers: Vec<SbDnsServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rules: Option<Vec<SbDnsRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#final: Option<String>,
}

#[derive(Serialize)]
struct SbDnsServer {
    #[serde(rename = "type")]
    typ: String,
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detour: Option<String>,
}

#[derive(Serialize)]
struct SbDnsRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    inbound: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
}

#[derive(Serialize)]
struct SbRoute {
    #[serde(skip_serializing_if = "Option::is_none")]
    rules: Option<Vec<SbRouteRule>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "final")]
    final_outbound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_detect_interface: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_domain_resolver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    find_process: Option<bool>,
}

#[derive(Serialize)]
struct SbRouteRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ip_cidr: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain_suffix: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain_keyword: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_name: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    process_path_regex: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outbound: Option<String>,
}

#[derive(Serialize)]
struct SbInbound {
    #[serde(rename = "type")]
    typ: String,
    tag: String,
    // SOCKS5 fields
    #[serde(skip_serializing_if = "Option::is_none")]
    listen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    listen_port: Option<u16>,
    // TUN fields
    #[serde(skip_serializing_if = "Option::is_none")]
    interface_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mtu: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_route: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strict_route: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
}

#[derive(Serialize)]
struct SbOutbound {
    #[serde(rename = "type")]
    typ: String,
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<SbTls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detour: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    udp_over_tcp: Option<SbUdpOverTcp>,
}

#[derive(Serialize)]
struct SbUdpOverTcp {
    enabled: bool,
}

#[derive(Serialize)]
struct SbTls {
    enabled: bool,
    server_name: String,
    insecure: bool,
}

pub struct ProxyManager {
    child: Arc<Mutex<Option<Child>>>,
    config_dir: PathBuf,
    config: Config,
    saved_dns: Arc<Mutex<Option<sysdns::DnsState>>>,
    active_mode: Arc<Mutex<Option<String>>>,
    pub debug_log_path: PathBuf,
}

impl ProxyManager {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let config_dir = ProjectDirs::from("com", "dakal-tls", "dakal-tls")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        fs::create_dir_all(&config_dir)?;

        Ok(ProxyManager {
            child: Arc::new(Mutex::new(None)),
            config_dir: config_dir.clone(),
            config,
            saved_dns: Arc::new(Mutex::new(None)),
            active_mode: Arc::new(Mutex::new(None)),
            debug_log_path: config_dir.join("dakal-tls-debug.log"),
        })
    }

    pub fn is_running(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *guard = None;
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn debug_log(&self, msg: impl AsRef<str>) {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{secs}] {}\n", msg.as_ref());
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.debug_log_path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn start(&mut self) -> Result<String> {
        if self.is_running() {
            bail!("Proxy already running");
        }

        // Check admin on Windows (needed for TUN)
        #[cfg(target_os = "windows")]
        {
            extern "system" {
                fn IsUserAnAdmin() -> i32;
            }
            // SAFETY: IsUserAnAdmin() from shell32.dll
            let is_admin = unsafe { IsUserAnAdmin() != 0 };
            if !is_admin {
                bail!("Admin required. Right-click stls.exe → 'Run as administrator'.");
            }
        }

        // Re-read config from active profile
        let store = crate::config::ProfileStore::load()?;
        self.config = store.get_active_config()?;
        self.debug_log(format!("config loaded"));

        let exe = self.get_bundled_or_download()?;
        self.debug_log(format!("sing-box exe: {}", exe.display()));

        let cfg = self.build_vpn_config()?;

        let cfg_json = serde_json::to_string_pretty(&cfg)?;
        let cfg_path = self.config_dir.join("config.json");

        let current_raw = fs::read_to_string(&cfg_path).ok();
        let current = current_raw.as_deref();
        if current != Some(&cfg_json) {
            fs::write(&cfg_path, &cfg_json)?;
            self.debug_log(format!("config written to {}", cfg_path.display()));
        } else {
            self.debug_log(format!("config unchanged, skipping write"));
        }

        // Validate config before launch (no window)
        self.debug_log("running sing-box check...");
        let mut cmd = Command::new(&exe);
        let check_output = no_window(&mut cmd)
            .arg("check")
            .arg("-c")
            .arg(&cfg_path)
            .output()
            .context("failed to run sing-box check")?;
        if !check_output.status.success() {
            let err_text = String::from_utf8_lossy(&check_output.stderr);
            let out_text = String::from_utf8_lossy(&check_output.stdout);
            self.debug_log(format!("config check FAILED: {err_text}{out_text}"));
            bail!(
                "Config validation failed:\n{}{}\nConfig: {}",
                err_text.trim(),
                out_text.trim(),
                cfg_path.display()
            );
        }
        self.debug_log("config check passed");

        let log_path = self.config_dir.join("sing-box.log");
        let log_file = fs::File::create(&log_path)?;

        self.debug_log("starting sing-box run...");
        // Start sing-box with hidden window on Windows
        #[cfg(target_os = "windows")]
        let child = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            Command::new(&exe)
                .arg("run")
                .arg("-c")
                .arg(&cfg_path)
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::from(log_file.try_clone()?))
                .stderr(Stdio::from(log_file))
                .spawn()?
        };
        self.debug_log("sing-box process spawned");

        #[cfg(not(target_os = "windows"))]
        let child = Command::new(&exe)
            .arg("run")
            .arg("-c")
            .arg(&cfg_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        *self.child.lock().unwrap() = Some(child);
        *self.active_mode.lock().unwrap() = Some("vpn".into());

        // Short wait — just enough to catch instant crash on first poll
        let mut guard = self.child.lock().unwrap();
        if let Some(ref mut c) = *guard {
            std::thread::sleep(std::time::Duration::from_millis(500));
            match c.try_wait() {
                Ok(Some(status)) => {
                    let log = fs::read_to_string(&log_path).unwrap_or_default();
                    self.debug_log(format!("sing-box exited (code={:?})", status.code()));
                    guard.take();
                    *self.active_mode.lock().unwrap() = None;
                    bail!("sing-box exited (code={:?}):\n{}", status.code(), log.trim());
                }
                Err(e) => {
                    self.debug_log(format!("sing-box check error: {e}"));
                    guard.take();
                    *self.active_mode.lock().unwrap() = None;
                    bail!("sing-box check failed: {e}");
                }
                Ok(None) => {}
            }
        }
        drop(guard);

        // Switch DNS to 8.8.8.8 so queries hit TUN
        match sysdns::DnsState::enable() {
            Ok(dns) => {
                *self.saved_dns.lock().unwrap() = Some(dns);
                self.debug_log("DNS set to 8.8.8.8");
            }
            Err(e) => {
                self.debug_log(format!("DNS set failed (non-fatal): {e}"));
            }
        }

        Ok("VPN mode started".to_string())
    }

    pub fn stop(&mut self) -> Result<String> {
        let _mode = self.active_mode.lock().unwrap().take();

        let mut guard = self.child.lock().unwrap();
        let was_running = guard.is_some();
        if let Some(mut child) = guard.take() {
            child.kill()?;
            child.wait()?;
        }
        drop(guard);

        if !was_running {
            bail!("Not running");
        }

        // Restore DNS (DHCP) — always in VPN-only mode
        let dns_state = self.saved_dns.lock().unwrap().take();
        if let Some(ref dns) = dns_state {
            let _ = dns.restore();
            self.debug_log("DNS restored to DHCP");
        }

        Ok("Stopped".into())
    }

    // ── VPN / TUN mode config ─────────────────────────────────────

    fn build_vpn_config(&self) -> Result<SbConfig> {
        let c = &self.config;

        // Resolve STLS server IP to bypass from TUN (prevents loop)
        // Fall back to hostname as-is if DNS fails — sing-box resolves at runtime
        let stls_ips: Vec<String> = resolve_hostname(&c.server_address).unwrap_or_else(|_| {
            eprintln!("[stls] DNS resolution failed for {} — using hostname directly", c.server_address);
            vec![]
        });

        let bypass_cidrs: Vec<String> = if stls_ips.is_empty() {
            vec!["198.18.0.0/15".into()]
        } else {
            stls_ips.iter().map(|ip| format!("{ip}/32")).collect()
        };

        // Use resolved IP in outbound server fields to avoid circular DNS
        let stls_ip = stls_ips.first()
            .map(|s| s.clone())
            .unwrap_or_else(|| "198.18.0.0".into());

        let mut outbounds = self.common_outbounds();
        for ob in &mut outbounds {
            if ob.tag == "ss-out" || ob.tag == "shadowtls-out" {
                ob.server = Some(stls_ip.clone());
            }
        }

        let mut cfg = SbConfig {
            log: SbLog {
                disabled: false,
                level: "info".into(),
                timestamp: true,
            },
            experimental: Some(SbExperimental {
                clash_api: Some(SbClashApi {
                    external_controller: "127.0.0.1:9097".into(),
                    secret: "dakal".into(),
                    default_mode: "rule".into(),
                }),
            }),
            dns: Some(SbDns {
                servers: vec![
                    SbDnsServer {
                        typ: "https".into(),
                        tag: "remote-doh".into(),
                        server: Some("1.1.1.1".into()),
                        server_port: None,
                        detour: Some("ss-out".into()),
                    },
                    SbDnsServer {
                        typ: "https".into(),
                        tag: "google-doh".into(),
                        server: Some("8.8.8.8".into()),
                        server_port: None,
                        detour: Some("ss-out".into()),
                    },
                ],
                rules: None,
                strategy: None,
                r#final: Some("remote-doh".into()),
            }),
            inbounds: vec![SbInbound {
                typ: "tun".into(),
                tag: "tun-in".into(),
                listen: None,
                listen_port: None,
                interface_name: None,
                address: Some(vec!["172.19.0.1/30".into()]),
                mtu: c.mtu,
                auto_route: Some(true),
                strict_route: Some(true),
                stack: Some("system".into()),
            }],
            outbounds,
            route: Some(SbRoute {
                rules: Some(vec![
                    SbRouteRule {
                        action: Some("sniff".into()),
                        protocol: None,
                        ip_cidr: None,
                        domain: None,
                        domain_suffix: None,
                        domain_keyword: None,
                        process_name: None,
                        process_path_regex: None,
                        outbound: None,
                    },
                    SbRouteRule {
                        action: Some("hijack-dns".into()),
                        protocol: Some("dns".into()),
                        ip_cidr: None,
                        domain: None,
                        domain_suffix: None,
                        domain_keyword: None,
                        process_name: None,
                        process_path_regex: None,
                        outbound: None,
                    },
                    SbRouteRule {
                        action: None,
                        protocol: None,
                        ip_cidr: Some(bypass_cidrs),
                        domain: None,
                        domain_suffix: None,
                        domain_keyword: None,
                        process_name: None,
                        process_path_regex: None,
                        outbound: Some("direct".into()),
                    },
                ]),
                final_outbound: Some("ss-out".into()),
                auto_detect_interface: Some(true),
                default_domain_resolver: Some("remote-doh".into()),
                find_process: Some(true),
            }),
        };

        // Add split tunnel rules if configured
        if !c.split_rules.is_empty() {
            let mut split_rules = Vec::new();
            for split_rule in &c.split_rules {
                let pattern = &split_rule.pattern;
                // Wildcard: *.example.com -> domain_suffix ".example.com"
                // Exact: example.com -> domain ["example.com"]
                // Keyword: contains "example" -> domain_keyword ["example"]
                if pattern.starts_with("*.") {
                    split_rules.push(SbRouteRule {
                        action: None,
                        protocol: None,
                        ip_cidr: None,
                        domain: None,
                        domain_suffix: Some(vec![pattern[1..].to_string()]),
                        domain_keyword: None,
                        process_name: None,
                        process_path_regex: None,
                        outbound: Some("direct".into()),
                    });
                } else if pattern.contains("*") {
                    split_rules.push(SbRouteRule {
                        action: None,
                        protocol: None,
                        ip_cidr: None,
                        domain: None,
                        domain_suffix: None,
                        domain_keyword: Some(vec![pattern.replace("*", "")]),
                        process_name: None,
                        process_path_regex: None,
                        outbound: Some("direct".into()),
                    });
                } else {
                    split_rules.push(SbRouteRule {
                        action: None,
                        protocol: None,
                        ip_cidr: None,
                        domain: Some(vec![pattern.clone()]),
                        domain_suffix: None,
                        domain_keyword: None,
                        process_name: None,
                        process_path_regex: None,
                        outbound: Some("direct".into()),
                    });
                }
            }
            cfg.route.as_mut().unwrap().rules.as_mut().unwrap().splice(2..2, split_rules);
        }

        Ok(cfg)
    }

    // ── shared outbounds (SS + STLS + direct) ─────────────────────

    fn common_outbounds(&self) -> Vec<SbOutbound> {
        let c = &self.config;
        vec![
            SbOutbound {
                typ: "shadowsocks".into(),
                tag: "ss-out".into(),
                server: Some(c.server_address.clone()),
                server_port: Some(c.ss_port),
                method: Some("2022-blake3-chacha20-poly1305".into()),
                password: Some(c.ss_password.clone()),
                version: None,
                tls: None,
                detour: Some("shadowtls-out".into()),
                // No udp field — sing-box 1.13.x rejects unknown fields on outbounds
                udp_over_tcp: Some(SbUdpOverTcp { enabled: true }),
            },
            SbOutbound {
                typ: "shadowtls".into(),
                tag: "shadowtls-out".into(),
                server: Some(c.server_address.clone()),
                server_port: Some(c.stls_port),
                version: Some(3),
                password: Some(c.stls_password.clone()),
                tls: Some(SbTls {
                    enabled: true,
                    server_name: c.stls_sni.clone(),
                    insecure: false,
                }),
                detour: None,
                // No udp field — sing-box 1.13.x rejects unknown fields on outbounds
                udp_over_tcp: None,
                method: None,
            },
            SbOutbound {
                typ: "direct".into(),
                tag: "direct".into(),
                server: None,
                server_port: None,
                method: None,
                password: None,
                version: None,
                tls: None,
                detour: None,
                // No udp field — sing-box 1.13.x rejects unknown fields on outbounds
                udp_over_tcp: None,
            },
        ]
    }

    // ── sing-box binary management ─────────────────────────────────

    fn sing_box_exe(&self) -> PathBuf {
        self.config_dir.join("sing-box.exe")
    }

    fn get_bundled_or_download(&self) -> Result<PathBuf> {
        let candidates = [
            // Next to exe
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("sing-box.exe"))),
            // Next to exe/resources (Tauri bundle layout)
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("resources").join("sing-box.exe"))),
            // Next to exe/bin (Tauri resources with bin/ prefix)
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("bin").join("sing-box.exe"))),
            // Relative paths
            Some(PathBuf::from("bin").join("sing-box.exe")),
            Some(PathBuf::from("sing-box.exe")),
            // Cached in config dir
            Some(self.sing_box_exe()),
        ];
        for path in candidates.iter().flatten() {
            if path.exists() {
                println!("[stls] using sing-box: {}", path.display());
                return Ok(path.clone());
            }
        }
        println!("[stls] no bundled sing-box found, downloading...");
        self.download_sing_box()
    }

    fn download_sing_box(&self) -> Result<PathBuf> {
        let exe = self.sing_box_exe();
        if exe.exists() {
            return Ok(exe);
        }

        println!("[stls] resolving latest sing-box release...");
        let client = reqwest::blocking::Client::builder()
            .user_agent("stls")
            .build()?;

        let rel: serde_json::Value = client
            .get("https://api.github.com/repos/SagerNet/sing-box/releases/latest")
            .send()?
            .json()?;

        let tag = rel["tag_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("no tag"))?;
        let version = tag.trim_start_matches('v');

        println!("[stls] downloading sing-box {version}...");
        let zip_name = format!("sing-box-{version}-windows-amd64.zip");
        let url =
            format!("https://github.com/SagerNet/sing-box/releases/download/{tag}/{zip_name}");

        let bytes = client.get(&url).send()?.error_for_status()?.bytes()?;

        println!("[stls] extracting...");
        let reader = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();
            if name.ends_with("sing-box.exe") {
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                let mut out = fs::File::create(&exe)?;
                out.write_all(&buf)?;
                println!("[stls] sing-box ready");
                return Ok(exe);
            }
        }

        bail!("sing-box.exe not found in release")
    }
}

// ── DNS resolver for STLS server IP (used to build TUN bypass) ────

fn resolve_hostname(host: &str) -> Result<Vec<String>> {
    let addr_str = format!("{host}:0");
    let addrs = addr_str
        .to_socket_addrs()
        .context("DNS resolution failed")?;
    let mut ips: Vec<String> = Vec::new();
    for addr in addrs {
        let ip = addr.ip().to_string();
        if !ips.contains(&ip) {
            ips.push(ip);
        }
    }
    if ips.is_empty() {
        bail!("no IPs resolved for {host}");
    }
    println!("[stls] resolved {host} -> {:?}", ips);
    Ok(ips)
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify VPN DNS config uses modern schema (type field, no legacy address).
    #[test]
    fn vpn_dns_uses_modern_schema() {
        let cfg = SbConfig {
            log: SbLog { disabled: false, level: "info".into(), timestamp: true },
            dns: Some(SbDns {
                servers: vec![
                    SbDnsServer {
                        typ: "tcp".into(),
                        tag: "dns-remote".into(),
                        server: Some("8.8.8.8".into()),
                        server_port: Some(53),
                        detour: None,
                    },
                ],
                rules: Some(vec![SbDnsRule {
                    inbound: None,
                    server: Some("dns-remote".into()),
                }]),
                strategy: Some("prefer_ipv4".into()),
            }),
            inbounds: vec![],
            outbounds: vec![],
            route: None,
        };

        let json = serde_json::to_value(&cfg).unwrap();
        let dns = json["dns"].as_object().unwrap();

        assert!(!dns.contains_key("independent_cache"));

        let servers = dns["servers"].as_array().unwrap();
        assert_eq!(servers.len(), 1);

        for server in servers {
            let typ = server["type"].as_str().unwrap();
            assert!(!server.contains_key("address"));
            assert!(!server.contains_key("transport"));
            assert!(!server.contains_key("detour"));

            match typ {
                "tcp" => {
                    assert!(server["server"].is_string());
                    assert!(server["server_port"].is_u64());
                }
                other => panic!("unexpected DNS server type: {other}"),
            }
        }
    }
}
