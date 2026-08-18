//! mDNS discovery of una servers advertising `_una._tcp.local.`.
//!
//! Used when `[server] url` is empty and `autodiscover = true`; a manually
//! configured URL always wins. The TXT record carries `version` and `api`.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const SERVICE_TYPE: &str = "_una._tcp.local.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredServer {
    /// Instance name, e.g. "una on studio".
    pub name: String,
    /// Base URL, e.g. "http://192.168.1.20:8765".
    pub url: String,
    pub version: Option<String>,
    pub api: Option<String>,
}

/// Browse for una servers for `timeout`, blocking the calling thread.
/// Call from `spawn_blocking` in async contexts.
pub fn discover(timeout: Duration) -> Vec<DiscoveredServer> {
    let daemon = match mdns_sd::ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("mdns: could not start daemon: {e}");
            return Vec::new();
        }
    };
    let receiver = match daemon.browse(SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("mdns: browse failed: {e}");
            let _ = daemon.shutdown();
            return Vec::new();
        }
    };

    let mut found: BTreeMap<String, DiscoveredServer> = BTreeMap::new();
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) => {
                if let Some(server) = server_from_info(&info) {
                    found.insert(server.url.clone(), server);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.stop_browse(SERVICE_TYPE);
    let _ = daemon.shutdown();
    found.into_values().collect()
}

fn server_from_info(info: &mdns_sd::ResolvedService) -> Option<DiscoveredServer> {
    let port = info.get_port();
    // Prefer IPv4 for a simpler URL; fall back to any address.
    let addrs: Vec<std::net::IpAddr> =
        info.get_addresses().iter().map(|a| a.to_ip_addr()).collect();
    let addr = addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())?;
    let host = match addr {
        std::net::IpAddr::V4(v4) => v4.to_string(),
        std::net::IpAddr::V6(v6) => format!("[{v6}]"),
    };
    let name = info
        .get_fullname()
        .strip_suffix(&format!(".{SERVICE_TYPE}"))
        .unwrap_or(info.get_fullname())
        .to_string();
    Some(DiscoveredServer {
        name,
        url: format!("http://{host}:{port}"),
        version: info.txt_properties.get_property_val_str("version").map(|s| s.to_string()),
        api: info.txt_properties.get_property_val_str("api").map(|s| s.to_string()),
    })
}
