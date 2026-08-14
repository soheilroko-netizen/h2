// doh.rs - Private DoH DNS toggle (independent of VPN connections)
// Mirrors set_doh_for_wow.bat: registers DoH templates + sets static DNS on
// the active adapter, or resets to DHCP + flushes when toggled off.
use anyhow::{bail, Result};
#[cfg(target_os = "windows")]
use anyhow::Context;

/// Primary DoH server (from set_doh_for_wow.bat)
const PRIMARY_IP: &str = "62.238.60.136";
const PRIMARY_DOH: &str = "https://fn.baft.uk/dns-query";
/// Secondary DoH server (from set_doh_for_wow.bat)
const SECONDARY_IP: &str = "5.255.116.43";
const SECONDARY_DOH: &str = "https://ns.reddeernook123.org/dns-query";

/// Run a netsh/ipconfig command without showing a console window.
/// Uses spawn_blocking so it doesn't block the tokio runtime / UI thread.
async fn run_hidden(args: Vec<&str>) -> Result<String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        // Extract the program name before moving args into closure
        let program = args[0].to_string();
        
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new(args[0])
                .args(&args[1..])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
        }).await;
        
        let output = match output {
            Ok(out) => out,
            Err(e) => return Err(anyhow::Error::new(e).context("spawn_blocking failed")),
        };
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("{} failed: {}", program, stderr));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = args;
        bail!("DoH DNS only supported on Windows")
    }
}

/// Detect the active network adapter name ("Enabled Connected" line).
pub async fn active_adapter() -> Result<String> {
    let out = run_hidden(vec!["netsh", "interface", "show", "interface"]).await?;
    for line in out.lines().skip(3) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("Enabled") && parts[1].eq_ignore_ascii_case("Connected") {
            let name = parts[3..].join(" ");
            return Ok(name);
        }
    }
    bail!("No active network adapter found")
}

/// Register a DoH template for a server IP.
async fn register_doh(server_ip: &str, template: &str) -> Result<()> {
    run_hidden(vec![
        "netsh", "dns", "add", "encryption",
        "server=", server_ip,
        "dohtemplate=", template,
        "autoupgrade=yes",
        "udpfallback=no",
    ]).await?;
    Ok(())
}

/// Remove a DoH template registration for a server IP.
async fn unregister_doh(server_ip: &str) -> Result<()> {
    run_hidden(vec!["netsh", "dns", "delete", "encryption", "server=", server_ip]).await?;
    Ok(())
}

/// Set DoH DNS on the active adapter (on).
pub async fn set_doh_dns() -> Result<String> {
    let iface = active_adapter().await?;
    register_doh(PRIMARY_IP, PRIMARY_DOH).await?;
    register_doh(SECONDARY_IP, SECONDARY_DOH).await?;

    run_hidden(vec!["netsh", "interface", "ipv4", "set", "dnsservers", "name=", &iface, "static", PRIMARY_IP, "primary"]).await?;
    run_hidden(vec!["netsh", "interface", "ipv4", "add", "dnsservers", "name=", &iface, SECONDARY_IP, "index=2"]).await?;
    run_hidden(vec!["ipconfig", "/flushdns"]).await?;
    Ok(iface)
}

/// Reset DNS to DHCP and flush (off).
pub async fn clear_doh_dns() -> Result<String> {
    let iface = active_adapter().await?;
    run_hidden(vec!["netsh", "interface", "ipv4", "set", "dnsservers", "name=", &iface, "dhcp"]).await?;
    // Clean up DoH template registrations (unused once DNS is DHCP)
    let _ = unregister_doh(PRIMARY_IP).await;
    let _ = unregister_doh(SECONDARY_IP).await;
    run_hidden(vec!["ipconfig", "/flushdns"]).await?;
    Ok(iface)
}

/// Whether the active adapter currently uses static (DoH) DNS.
pub async fn doh_active() -> Result<bool> {
    let iface = active_adapter().await?;
    let out = run_hidden(vec!["netsh", "interface", "ipv4", "show", "dnsservers", "name=", &iface]).await?;
    // Static config shows "Configured DNS servers" with a listed address;
    // DHCP shows "DHCP-configured DNS servers".
    Ok(out.contains("Configured DNS servers"))
}