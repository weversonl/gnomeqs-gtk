use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::{Duration, Instant, sleep};
use tokio_util::sync::CancellationToken;

use crate::{EndpointInfo, EndpointTransport};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WifiDirectBackend {
    NetworkManager,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WifiDirectStatus {
    BackendMissing,
    BackendNotRunning,
    BackendQueryFailed,
    NoWifiInterface,
    WifiInterfaceUnavailable,
    NoP2pInterface,
    P2pInterfaceUnavailable,
    P2pInterfaceAvailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WifiDirectCapability {
    pub backend: Option<WifiDirectBackend>,
    pub status: WifiDirectStatus,
    pub available: bool,
    pub p2p_interface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WifiDirectSessionInfo {
    pub p2p_interface: String,
    pub connection_name: Option<String>,
    pub ip_interface: Option<String>,
    pub ipv4_addresses: Vec<String>,
    pub peer_ipv4_candidates: Vec<String>,
    pub wifi_connected: bool,
}

impl WifiDirectCapability {
    fn unavailable(status: WifiDirectStatus) -> Self {
        Self {
            backend: None,
            status,
            available: false,
            p2p_interface: None,
        }
    }

    fn network_manager(
        status: WifiDirectStatus,
        available: bool,
        p2p_interface: Option<String>,
    ) -> Self {
        Self {
            backend: Some(WifiDirectBackend::NetworkManager),
            status,
            available,
            p2p_interface,
        }
    }
}

pub fn detect_wifi_direct_capability() -> WifiDirectCapability {
    #[cfg(target_os = "linux")]
    {
        detect_linux_network_manager()
    }

    #[cfg(not(target_os = "linux"))]
    {
        WifiDirectCapability::unavailable(WifiDirectStatus::BackendMissing)
    }
}

#[cfg(target_os = "linux")]
pub async fn run_wifi_direct_discovery(
    sender: broadcast::Sender<EndpointInfo>,
    ctk: CancellationToken,
) -> Result<(), anyhow::Error> {
    let capability = detect_linux_network_manager();
    if capability.status != WifiDirectStatus::P2pInterfaceAvailable {
        info!(
            "Wi-Fi Direct discovery skipped: status={:?} interface={:?}",
            capability.status,
            capability.p2p_interface
        );
        return Ok(());
    }

    let interface = capability
        .p2p_interface
        .clone()
        .ok_or_else(|| anyhow!("missing Wi-Fi Direct interface"))?;
    let object_path = network_manager_device_path(&interface)?;
    info!(
        "Wi-Fi Direct discovery starting on interface={} object_path={}",
        interface,
        object_path
    );

    match run_gdbus(
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.NetworkManager",
            "--object-path",
            &object_path,
            "--method",
            "org.freedesktop.NetworkManager.Device.WifiP2P.StartFind",
            "{}",
        ],
    ) {
        Ok(output) => info!("Wi-Fi Direct StartFind response: {}", output),
        Err(e) => warn!("Wi-Fi Direct StartFind failed: {e}"),
    }

    if let Ok(snapshot) = query_wifi_direct_device_snapshot(&interface) {
        info!("Wi-Fi Direct device snapshot: {}", snapshot);
    }

    let mut known: HashMap<String, EndpointInfo> = HashMap::new();
    let mut last_poll = Instant::now() - Duration::from_secs(3);
    let mut signal_requested_refresh = false;
    let mut signal_monitor = spawn_wifi_direct_signal_monitor(&object_path)
        .map_err(|e| {
            warn!("Wi-Fi Direct signal monitor failed to start: {e}");
            e
        })
        .ok();

    loop {
        if let Some((_, rx)) = signal_monitor.as_mut() {
            while let Ok(line) = rx.try_recv() {
                if line.contains("PeerAdded") || line.contains("PeerRemoved") {
                    info!("Wi-Fi Direct D-Bus signal: {}", line.trim());
                    signal_requested_refresh = true;
                }
            }
        }

        if signal_requested_refresh || last_poll.elapsed() >= Duration::from_secs(2) {
            signal_requested_refresh = false;
            last_poll = Instant::now();

            match query_wifi_direct_peers(&object_path) {
                Ok(peers) => {
                    let mut current: HashSet<String> = HashSet::new();

                    for peer in peers {
                        current.insert(peer.id.clone());
                        if !known.contains_key(&peer.id) {
                            info!(
                                "Wi-Fi Direct peer discovered: id={} name={:?} mac={:?} path={:?}",
                                peer.id,
                                peer.name,
                                peer.wifi_direct_peer_mac,
                                peer.wifi_direct_peer_path
                            );
                            let _ = sender.send(peer.clone());
                        }
                        known.insert(peer.id.clone(), peer);
                    }

                    let removed: Vec<String> = known
                        .keys()
                        .filter(|id| !current.contains(*id))
                        .cloned()
                        .collect();

                    for id in removed {
                        info!("Wi-Fi Direct peer removed: id={id}");
                        known.remove(&id);
                        let _ = sender.send(EndpointInfo {
                            id,
                            present: Some(false),
                            ..Default::default()
                        });
                    }
                }
                Err(e) => warn!("Wi-Fi Direct peer query failed: {e}"),
            }
        }

        tokio::select! {
            _ = ctk.cancelled() => {
                break;
            }
            _ = sleep(Duration::from_millis(300)) => {}
        }
    }

    if let Some((child, _)) = signal_monitor.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    for id in known.keys().cloned().collect::<Vec<_>>() {
        let _ = sender.send(EndpointInfo {
            id,
            present: Some(false),
            ..Default::default()
        });
    }

    if let Err(e) = run_gdbus(
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.NetworkManager",
            "--object-path",
            &object_path,
            "--method",
            "org.freedesktop.NetworkManager.Device.WifiP2P.StopFind",
        ],
    ) {
        warn!("Wi-Fi Direct StopFind failed: {e}");
    }

    info!("Wi-Fi Direct discovery stopped on interface={interface}");

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub async fn run_wifi_direct_discovery(
    _sender: broadcast::Sender<EndpointInfo>,
    _ctk: CancellationToken,
) -> Result<(), anyhow::Error> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn detect_linux_network_manager() -> WifiDirectCapability {
    let nmcli = match find_in_path("nmcli") {
        Some(path) => path,
        None => {
            return WifiDirectCapability::unavailable(WifiDirectStatus::BackendMissing);
        }
    };

    match run_nmcli(&nmcli, &["-t", "-f", "RUNNING", "general"]) {
        Ok(output) if output.trim() == "running" => {}
        Ok(_) => {
            return WifiDirectCapability::unavailable(WifiDirectStatus::BackendNotRunning);
        }
        Err(_) => {
            return WifiDirectCapability::unavailable(WifiDirectStatus::BackendQueryFailed);
        }
    }

    match run_nmcli(&nmcli, &["-t", "-f", "DEVICE,TYPE,STATE", "device"]) {
        Ok(output) => {
            let mut has_wifi = false;
            let mut has_wifi_candidate = false;
            let mut p2p_device_name: Option<String> = None;
            let mut p2p_usable = false;

            for line in output.lines() {
                let mut parts = line.split(':');
                let dev_name = parts.next().unwrap_or_default();
                let dev_type = parts.next().unwrap_or_default();
                let dev_state = parts.next().unwrap_or_default();
                if dev_type == "wifi" {
                    has_wifi = true;
                    if !matches!(dev_state, "unavailable" | "unknown" | "unmanaged") {
                        has_wifi_candidate = true;
                    }
                } else if dev_type == "wifi-p2p" {
                    p2p_device_name = Some(dev_name.to_string());
                    if !matches!(dev_state, "unavailable" | "unknown" | "unmanaged") {
                        p2p_usable = true;
                    }
                }
            }

            if let Some(p2p_name) = p2p_device_name {
                if p2p_usable {
                    WifiDirectCapability::network_manager(
                        WifiDirectStatus::P2pInterfaceAvailable,
                        true,
                        Some(p2p_name),
                    )
                } else {
                    WifiDirectCapability::network_manager(
                        WifiDirectStatus::P2pInterfaceUnavailable,
                        false,
                        Some(p2p_name),
                    )
                }
            } else if !has_wifi {
                WifiDirectCapability::unavailable(WifiDirectStatus::NoWifiInterface)
            } else if !has_wifi_candidate {
                WifiDirectCapability::network_manager(
                    WifiDirectStatus::WifiInterfaceUnavailable,
                    false,
                    None,
                )
            } else {
                WifiDirectCapability::network_manager(
                    WifiDirectStatus::NoP2pInterface,
                    false,
                    None,
                )
            }
        }
        Err(_) => WifiDirectCapability::unavailable(WifiDirectStatus::BackendQueryFailed),
    }
}

#[cfg(target_os = "linux")]
fn network_manager_device_path(interface: &str) -> Result<String, anyhow::Error> {
    let nmcli = find_in_path("nmcli").ok_or_else(|| anyhow!("nmcli was not found"))?;
    let output = run_nmcli(
        &nmcli,
        &[
            "-g",
            "GENERAL.DEVICE,GENERAL.DBUS-PATH",
            "device",
            "show",
            interface,
        ],
    )?;
    let mut lines = output.lines();
    let _ = lines.next();
    lines.next()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .ok_or_else(|| anyhow!("NetworkManager did not expose a D-Bus path for {interface}"))
}

#[cfg(target_os = "linux")]
fn query_wifi_direct_peers(device_path: &str) -> Result<Vec<EndpointInfo>, anyhow::Error> {
    let output = run_gdbus(&[
        "call",
        "--system",
        "--dest",
        "org.freedesktop.NetworkManager",
        "--object-path",
        device_path,
        "--method",
        "org.freedesktop.DBus.Properties.Get",
        "org.freedesktop.NetworkManager.Device.WifiP2P",
        "Peers",
    ])?;

    info!("Wi-Fi Direct raw Peers response: {}", output);

    let peer_paths = extract_object_paths(&output);
    let mut peers = Vec::with_capacity(peer_paths.len());

    for peer_path in peer_paths {
        peers.push(query_wifi_direct_peer(&peer_path));
    }

    Ok(peers)
}

#[cfg(target_os = "linux")]
fn query_wifi_direct_device_snapshot(interface: &str) -> Result<String, anyhow::Error> {
    let nmcli = find_in_path("nmcli").ok_or_else(|| anyhow!("nmcli was not found"))?;
    let output = run_nmcli(
        &nmcli,
        &[
            "-f",
            "GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION,GENERAL.DBUS-PATH",
            "device",
            "show",
            interface,
        ],
    )?;
    Ok(output.replace('\n', " | "))
}

#[cfg(target_os = "linux")]
fn query_wifi_direct_peer(peer_path: &str) -> EndpointInfo {
    let output = run_gdbus(&[
        "call",
        "--system",
        "--dest",
        "org.freedesktop.NetworkManager",
        "--object-path",
        peer_path,
        "--method",
        "org.freedesktop.DBus.Properties.GetAll",
        "org.freedesktop.NetworkManager.WifiP2PPeer",
    ])
    .unwrap_or_default();

    let hw_address = extract_gdbus_string_property(&output, "HwAddress");
    let name = extract_gdbus_string_property(&output, "Name");
    let manufacturer = extract_gdbus_string_property(&output, "Manufacturer");
    let model = extract_gdbus_string_property(&output, "Model");
    let strength = extract_gdbus_u8_property(&output, "Strength");

    let display_name = name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| model.clone().filter(|value| !value.trim().is_empty()))
        .or_else(|| manufacturer.clone().filter(|value| !value.trim().is_empty()))
        .or_else(|| hw_address.clone());

    let peer_id_suffix = hw_address
        .clone()
        .unwrap_or_else(|| {
            peer_path
                .rsplit('/')
                .next()
                .unwrap_or("unknown")
                .to_string()
        });

    let mut label = display_name.unwrap_or_else(|| "Wi-Fi Direct peer".to_string());
    if let Some(strength) = strength {
        label = format!("{label} ({strength}%)");
    }

    EndpointInfo {
        fullname: peer_path.to_string(),
        id: format!("wifi-direct:{peer_id_suffix}"),
        name: Some(label),
        ip: None,
        port: None,
        rtype: None,
        present: Some(true),
        transport: Some(EndpointTransport::WifiDirectPeer),
        wifi_direct_peer_path: Some(peer_path.to_string()),
        wifi_direct_peer_mac: hw_address,
    }
}

#[cfg(target_os = "linux")]
pub async fn activate_wifi_direct_peer(peer_mac: &str) -> Result<(), anyhow::Error> {
    let capability = detect_linux_network_manager();
    if capability.status != WifiDirectStatus::P2pInterfaceAvailable {
        return Err(anyhow!(
            "Wi-Fi Direct is not currently available through NetworkManager"
        ));
    }

    let interface = capability
        .p2p_interface
        .ok_or_else(|| anyhow!("missing Wi-Fi Direct interface"))?;
    let nmcli = find_in_path("nmcli").ok_or_else(|| anyhow!("nmcli was not found"))?;
    let connection_name = format!(
        "gnomeqs-wifi-direct-{}",
        peer_mac
            .chars()
            .filter(|ch| ch.is_ascii_hexdigit())
            .collect::<String>()
            .to_lowercase()
    );

    info!(
        "Wi-Fi Direct activation requested: peer_mac={} interface={} connection_name={}",
        peer_mac,
        interface,
        connection_name
    );

    let _ = Command::new(&nmcli)
        .args(["connection", "delete", &connection_name])
        .output();

    let output = Command::new(&nmcli)
        .args([
            "connection",
            "add",
            "type",
            "wifi-p2p",
            "ifname",
            &interface,
            "peer",
            peer_mac,
            "autoconnect",
            "no",
            "save",
            "no",
            "con-name",
            &connection_name,
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "NetworkManager failed to activate Wi-Fi Direct session: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    info!(
        "Wi-Fi Direct activation command accepted: peer_mac={} connection_name={}",
        peer_mac,
        connection_name
    );

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub async fn activate_wifi_direct_peer(_peer_mac: &str) -> Result<(), anyhow::Error> {
    Err(anyhow!("Wi-Fi Direct is only supported on Linux"))
}

#[cfg(target_os = "linux")]
pub async fn wait_for_wifi_direct_session(
    timeout: Duration,
) -> Result<Option<WifiDirectSessionInfo>, anyhow::Error> {
    let capability = detect_linux_network_manager();
    if capability.status != WifiDirectStatus::P2pInterfaceAvailable {
        return Ok(None);
    }

    let interface = capability
        .p2p_interface
        .ok_or_else(|| anyhow!("missing Wi-Fi Direct interface"))?;

    let started = std::time::Instant::now();
    let mut last_snapshot: Option<WifiDirectSessionInfo> = None;
    while started.elapsed() < timeout {
        let session = current_wifi_direct_session(&interface)?;
        if last_snapshot.as_ref() != Some(&session) {
            info!(
                "Wi-Fi Direct session state: p2p_interface={} connection={:?} ip_interface={:?} ipv4={:?} peers={:?} wifi_connected={}",
                session.p2p_interface,
                session.connection_name,
                session.ip_interface,
                session.ipv4_addresses,
                session.peer_ipv4_candidates,
                session.wifi_connected
            );
            last_snapshot = Some(session.clone());
        }
        if session.connection_name.is_some()
            || !session.ipv4_addresses.is_empty()
            || !session.peer_ipv4_candidates.is_empty()
        {
            return Ok(Some(session));
        }
        sleep(Duration::from_millis(800)).await;
    }

    Ok(Some(current_wifi_direct_session(&interface)?))
}

#[cfg(not(target_os = "linux"))]
pub async fn wait_for_wifi_direct_session(
    _timeout: Duration,
) -> Result<Option<WifiDirectSessionInfo>, anyhow::Error> {
    Ok(None)
}

#[cfg(target_os = "linux")]
fn run_gdbus(args: &[&str]) -> Result<String, anyhow::Error> {
    let gdbus = find_in_path("gdbus").ok_or_else(|| anyhow!("gdbus was not found"))?;
    let output = Command::new(gdbus).args(args).output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "gdbus call failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn spawn_wifi_direct_signal_monitor(
    device_path: &str,
) -> Result<(Child, mpsc::Receiver<String>), anyhow::Error> {
    let gdbus = find_in_path("gdbus").ok_or_else(|| anyhow!("gdbus was not found"))?;
    let mut child = Command::new(gdbus)
        .args([
            "monitor",
            "--system",
            "--dest",
            "org.freedesktop.NetworkManager",
            "--object-path",
            device_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture gdbus monitor stdout"))?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok((child, rx))
}

#[cfg(target_os = "linux")]
fn current_wifi_direct_session(interface: &str) -> Result<WifiDirectSessionInfo, anyhow::Error> {
    let nmcli = find_in_path("nmcli").ok_or_else(|| anyhow!("nmcli was not found"))?;
    let output = run_nmcli(
        &nmcli,
        &[
            "-t",
            "-f",
            "GENERAL.CONNECTION,GENERAL.IP-IFACE,IP4.ADDRESS",
            "device",
            "show",
            interface,
        ],
    )?;

    let mut connection_name = None;
    let mut ip_interface = None;
    let mut ipv4_addresses = Vec::new();

    for line in output.lines() {
        if let Some(value) = line.strip_prefix("GENERAL.CONNECTION:") {
            let value = value.trim();
            if !value.is_empty() && value != "--" {
                connection_name = Some(value.to_string());
            }
            continue;
        }

        if let Some(value) = line.strip_prefix("GENERAL.IP-IFACE:") {
            let value = value.trim();
            if !value.is_empty() && value != "--" {
                ip_interface = Some(value.to_string());
            }
            continue;
        }

        if line.starts_with("IP4.ADDRESS") {
            let address = line
                .split_once(':')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            if !address.is_empty() && address != "--" {
                ipv4_addresses.push(address.to_string());
            }
        }
    }

    let peer_ipv4_candidates = match ip_interface.as_deref() {
        Some(ip_interface) => query_wifi_direct_neighbor_candidates(ip_interface)?,
        None => Vec::new(),
    };

    Ok(WifiDirectSessionInfo {
        p2p_interface: interface.to_string(),
        connection_name,
        ip_interface,
        ipv4_addresses,
        peer_ipv4_candidates,
        wifi_connected: has_connected_wifi_uplink()?,
    })
}

#[cfg(target_os = "linux")]
fn query_wifi_direct_neighbor_candidates(interface: &str) -> Result<Vec<String>, anyhow::Error> {
    let ip = find_in_path("ip").ok_or_else(|| anyhow!("ip was not found"))?;
    let output = Command::new(ip)
        .args(["-4", "neigh", "show", "dev", interface])
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut candidates = Vec::new();
    for line in stdout.lines() {
        let Some(address) = line.split_whitespace().next() else {
            continue;
        };
        if address.parse::<std::net::Ipv4Addr>().is_ok() {
            candidates.push(address.to_string());
        }
    }
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

#[cfg(target_os = "linux")]
fn has_connected_wifi_uplink() -> Result<bool, anyhow::Error> {
    let nmcli = find_in_path("nmcli").ok_or_else(|| anyhow!("nmcli was not found"))?;
    let output = run_nmcli(&nmcli, &["-t", "-f", "DEVICE,TYPE,STATE", "device"])?;
    Ok(output.lines().any(|line| {
        let mut parts = line.split(':');
        let _device = parts.next();
        let dev_type = parts.next().unwrap_or_default();
        let state = parts.next().unwrap_or_default();
        dev_type == "wifi" && state == "connected"
    }))
}

#[cfg(target_os = "linux")]
fn extract_object_paths(output: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = output;
    while let Some(idx) = rest.find("/org/freedesktop/") {
        let candidate = &rest[idx..];
        let end = candidate
            .find(['\'', ']', ',', ' ', ')', '>'])
            .unwrap_or(candidate.len());
        let path = candidate[..end].trim();
        if !path.is_empty() {
            paths.push(path.to_string());
        }
        rest = &candidate[end..];
    }
    paths
}

#[cfg(target_os = "linux")]
fn extract_gdbus_string_property(output: &str, property: &str) -> Option<String> {
    let needle = format!("'{property}': <'");
    let start = output.find(&needle)? + needle.len();
    let remaining = &output[start..];
    let end = remaining.find("'>")?;
    Some(remaining[..end].to_string())
}

#[cfg(target_os = "linux")]
fn extract_gdbus_u8_property(output: &str, property: &str) -> Option<u8> {
    let needle = format!("'{property}': <byte ");
    let start = output.find(&needle)? + needle.len();
    let remaining = &output[start..];
    let end = remaining.find('>')?;
    remaining[..end].trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn run_nmcli(binary: &Path, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(binary).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "linux")]
fn find_in_path(binary: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}
