//! discovery.rs
//! Find the receivers on the local network with mDNS. Used by `apcast list`, by `--device`
//! and by the menu's list.
//!
//! Author: O. Booklage
//! Date: October 2026
//! Licence: MIT

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use mdns_sd::{RecvTimeoutError, ResolvedService, ServiceDaemon, ServiceEvent};

/// Receivers that take video. Audio-only receivers use another type, not browsed.
const SERVICE_TYPE: &str = "_airplay._tcp.local.";

/// A receiver, reached on its control port `port`.
#[derive(Debug, Clone)]
pub struct CastDevice {
    pub id: String,
    pub name: String,
    pub model: String,
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl CastDevice {
    fn from_service(info: &ResolvedService) -> Option<Self> {
        let txt = |key: &str| info.get_property_val_str(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let instance = info.get_fullname().strip_suffix(SERVICE_TYPE).map(|n| n.trim_end_matches('.'));
        let ip = pick_address(info.get_addresses_v4().into_iter().collect(), local_address());
        Self::from_txt(txt, instance, ip, info.get_port())
    }

    /// Receiver from its TXT record, its instance name, its address (A record) and its SRV port.
    fn from_txt(txt: impl Fn(&str) -> Option<String>, instance: Option<&str>, ip: Option<Ipv4Addr>, port: u16) -> Option<Self> {
        let ip = ip?;
        let name = instance.filter(|n| !n.is_empty()).map_or_else(|| ip.to_string(), str::to_string);
        // deviceid is the receiver's MAC address: stable across address changes.
        let id = txt("deviceid").unwrap_or_else(|| format!("{ip}:{port}"));
        let model = txt("model").unwrap_or_else(|| "Receiver".into());
        Some(Self { id, name, model, ip, port })
    }
}

/// The PC's address on its network: the one a packet to the Internet would leave from.
/// Nothing is sent, connecting a UDP socket only picks the route.
fn local_address() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        std::net::IpAddr::V6(_) => None,
    }
}

/// Of the addresses a receiver announces, the one on the PC's /24, else the lowest one.
/// UxPlay on this PC announced the Docker bridge address too, which a school PC cannot use.
/// ponytail: a /24 guess, read the interfaces' real masks if a school network is wider.
fn pick_address(mut addresses: Vec<Ipv4Addr>, local: Option<Ipv4Addr>) -> Option<Ipv4Addr> {
    addresses.sort();
    let same_network = |ip: &&Ipv4Addr| local.is_some_and(|local| ip.octets()[..3] == local.octets()[..3]);
    addresses.iter().find(same_network).or(addresses.first()).copied()
}

/// Browse the network for `timeout` and return the receivers found, sorted by name.
pub fn discover(timeout: Duration) -> anyhow::Result<Vec<CastDevice>> {
    let mut found = BTreeMap::new();
    browse(timeout, |device| {
        // Keyed by id: a receiver answers several times during the browse.
        found.insert(device.id.clone(), device);
        false
    })?;
    let mut devices: Vec<_> = found.into_values().collect();
    devices.sort_by_key(|d| d.name.to_lowercase());
    Ok(devices)
}

/// Find the receiver whose name (case-insensitive) or id is `query`.
/// Returns as soon as it answers, instead of waiting for the full timeout.
/// When it does not answer, the error lists the receivers seen during the same browse.
pub fn find(query: &str, timeout: Duration) -> anyhow::Result<CastDevice> {
    let mut hit = None;
    let mut seen = BTreeMap::new();
    browse(timeout, |device| {
        if is_named(&device, query) {
            hit = Some(device);
            return true;
        }
        seen.insert(device.id.clone(), device.name);
        false
    })?;
    if let Some(device) = hit {
        return Ok(device);
    }
    if seen.is_empty() {
        anyhow::bail!("{NONE_FOUND}");
    }
    let names: Vec<_> = seen.into_values().collect();
    anyhow::bail!("receiver {query:?} not found; available: {}", names.join(", "))
}

/// True when `query` is the receiver's id, or its name in any case (accented letters included).
fn is_named(device: &CastDevice, query: &str) -> bool {
    device.id == query || device.name.to_lowercase() == query.to_lowercase()
}

pub const NONE_FOUND: &str = "no receiver found (check that mDNS/UDP 5353 is not blocked)";

/// Call `on_device` for each resolved receiver until it returns true or `timeout` expires.
pub fn browse(timeout: Duration, mut on_device: impl FnMut(CastDevice) -> bool) -> anyhow::Result<()> {
    let mdns = ServiceDaemon::new()?;
    let browser = mdns.browse(SERVICE_TYPE)?;
    let deadline = Instant::now() + timeout;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match browser.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                tracing::debug!(?info, "resolved");
                if CastDevice::from_service(&info).is_some_and(&mut on_device) {
                    break;
                }
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = mdns.shutdown();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_names_in_any_case() {
        let device = CastDevice {
            id: "AA:BB:CC:DD:EE:FF".into(),
            name: "TÉLÉ Salle 12".into(),
            model: "Receiver".into(),
            ip: Ipv4Addr::LOCALHOST,
            port: 7000,
        };
        assert!(is_named(&device, "télé salle 12"));
        assert!(is_named(&device, "AA:BB:CC:DD:EE:FF"));
        assert!(!is_named(&device, "Salle 12"));
    }

    #[test]
    fn reads_a_txt_record() {
        let record = [("deviceid", "AA:BB:CC:DD:EE:FF"), ("model", "TV1,1"), ("srcvers", "377.40.00")];
        let txt = |key: &str| record.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string());
        let device = CastDevice::from_txt(txt, Some("Salle 104"), Some(Ipv4Addr::new(10, 0, 0, 7)), 7000).unwrap();
        assert_eq!((device.id.as_str(), device.name.as_str(), device.model.as_str()), ("AA:BB:CC:DD:EE:FF", "Salle 104", "TV1,1"));
        // Without TXT keys nor instance name: the address as name and id.
        let bare = CastDevice::from_txt(|_| None, None, Some(Ipv4Addr::new(10, 0, 0, 8)), 7000).unwrap();
        assert_eq!((bare.name.as_str(), bare.id.as_str(), bare.model.as_str()), ("10.0.0.8", "10.0.0.8:7000", "Receiver"));
        assert!(CastDevice::from_txt(|_| None, None, None, 7000).is_none());
    }

    #[test]
    fn picks_the_address_on_the_pc_network() {
        let (docker, lan) = (Ipv4Addr::new(172, 17, 0, 1), Ipv4Addr::new(192, 168, 33, 112));
        let local = Some(Ipv4Addr::new(192, 168, 33, 20));
        assert_eq!(pick_address(vec![docker, lan], local), Some(lan));
        // Nothing on the PC's network: the lowest address, the same at every browse.
        assert_eq!(pick_address(vec![lan, docker], Some(Ipv4Addr::new(10, 0, 0, 5))), Some(docker));
        assert_eq!(pick_address(vec![], local), None);
    }
}
