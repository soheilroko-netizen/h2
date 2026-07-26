// sysdns.rs - Windows DNS management via netsh (no flashing cmd windows)
// Saves/restores DNS servers per interface. No deps.

use anyhow::{Context, Result};
use std::process::Command;

const DNS_IP: &str = "8.8.8.8";

#[cfg(target_os = "windows")]
fn no_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}
#[cfg(not(target_os = "windows"))]
fn no_window(cmd: &mut Command) -> &mut Command { cmd }

pub struct DnsState {
    pub enabled: bool,
}

impl DnsState {
    /// Parse current DNS from all active interfaces, store, then set to 8.8.8.8
    pub fn enable() -> Result<Self> {
        save_dns_config()?;
        set_dns(DNS_IP)?;
        Ok(DnsState { enabled: true })
    }

    /// Revert to DHCP on all interfaces
    pub fn restore(&self) -> Result<()> {
        restore_dns()?;
        Ok(())
    }
}

fn get_active_interfaces() -> Result<Vec<String>> {
    let out = no_window(
        Command::new("netsh")
            .arg("interface")
            .arg("ip")
            .arg("show")
            .arg("interfaces")
    )
        .output()
        .context("netsh show interfaces failed")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut ifaces = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("connected") && !trimmed.contains("Loopback") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let after_idx = trimmed.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '\t');
                let name = after_idx
                    .split("connected")
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches(|c| c == ' ' || c == '\t' || c == '.');
                if !name.is_empty() {
                    ifaces.push(name.to_string());
                }
            }
        }
    }
    if ifaces.is_empty() {
        ifaces.push("Local Area Connection".into());
    }
    Ok(ifaces)
}

fn save_dns_config() -> Result<()> {
    // No-op: restore just sets DHCP, doesn't need saved state
    Ok(())
}

fn set_dns(dns: &str) -> Result<()> {
    let ifaces = get_active_interfaces()?;
    for name in &ifaces {
        let status = no_window(
            Command::new("netsh")
                .arg("interface")
                .arg("ip")
                .arg("set")
                .arg("dns")
                .arg(name)
                .arg("static")
                .arg(dns)
        )
            .status()
            .context(format!("failed to set DNS on {name}"))?;
        if !status.success() {
            eprintln!("[stls] warning: failed to set DNS on {name}");
        }
    }
    Ok(())
}

fn restore_dns() -> Result<()> {
    let ifaces = get_active_interfaces()?;
    for name in &ifaces {
        let status = no_window(
            Command::new("netsh")
                .arg("interface")
                .arg("ip")
                .arg("set")
                .arg("dns")
                .arg(name)
                .arg("dhcp")
        )
            .status()
            .context(format!("failed to restore DNS on {name}"))?;
        if !status.success() {
            eprintln!("[stls] warning: failed to restore DNS on {name}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_get_interfaces() {
        let ifaces = get_active_interfaces().unwrap();
        println!("interfaces: {:?}", ifaces);
        assert!(!ifaces.is_empty());
    }
}
