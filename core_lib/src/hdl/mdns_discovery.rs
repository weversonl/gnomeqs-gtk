use std::collections::HashMap;
use std::time::Instant;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::time::{Duration, interval};
use tokio_util::sync::CancellationToken;

use crate::utils::{is_not_self_ip, parse_mdns_endpoint_info};
use crate::DeviceType;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum EndpointTransport {
    #[default]
    MdnsTcp,
    WifiDirectPeer,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct EndpointInfo {
    pub fullname: String,
    pub id: String,
    pub name: Option<String>,
    pub ip: Option<String>,
    pub port: Option<String>,
    pub rtype: Option<DeviceType>,
    pub present: Option<bool>,
    pub transport: Option<EndpointTransport>,
    pub wifi_direct_peer_path: Option<String>,
    pub wifi_direct_peer_mac: Option<String>,
}

pub struct MDnsDiscovery {
    daemon: ServiceDaemon,
    sender: broadcast::Sender<EndpointInfo>,
}

impl MDnsDiscovery {
    pub fn new(sender: broadcast::Sender<EndpointInfo>) -> Result<Self, anyhow::Error> {
        let daemon = ServiceDaemon::new()?;

        Ok(Self { daemon, sender })
    }

    pub async fn run(self, ctk: CancellationToken) -> Result<(), anyhow::Error> {
        info!("MDnsDiscovery: service starting");
        const REMOVAL_GRACE_PERIOD: Duration = Duration::from_secs(4);

        let service_type = "_FC9F5ED42C8A._tcp.local.";
        let receiver = self.daemon.browse(service_type)?;
        let mut cleanup_tick = interval(Duration::from_millis(800));

        // Map with fullname as key and EndpointInfo as value
        let mut cache: HashMap<String, EndpointInfo> = HashMap::new();
        let mut pending_removals: HashMap<String, Instant> = HashMap::new();

        loop {
            tokio::select! {
                _ = ctk.cancelled() => {
                    info!("MDnsDiscovery: tracker cancelled, breaking");
                    break;
                }
                _ = cleanup_tick.tick() => {
                    let now = Instant::now();
                    let expired: Vec<String> = pending_removals
                        .iter()
                        .filter_map(|(fullname, deadline)| {
                            if *deadline <= now {
                                Some(fullname.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    for fullname in expired {
                        let should_remove = cache.get(&fullname).map(|ei| ei.id.clone());
                        pending_removals.remove(&fullname);

                        if let Some(id) = should_remove {
                            info!("ServiceRemoved: Remove a previous service after grace period: {}", fullname);
                            cache.remove(&fullname);
                            let _ = self.sender.send(EndpointInfo {
                                id,
                                present: Some(false),
                                ..Default::default()
                            });
                        }
                    }
                }
                r = receiver.recv_async() => {
                    match r {
                        Ok(event) => {
                            match event {
                                ServiceEvent::ServiceResolved(info) => {
                                    let port = info.get_port();

                                    let ip_hash = info.get_addresses_v4();
                                    if ip_hash.is_empty() {
                                        continue;
                                    }

                                    let ip = match ip_hash.iter().next() {
                                        Some(i) => i,
                                        None => continue,
                                    };

                                    // Check that the IP is not a "self IP"
                                    if !is_not_self_ip(ip) {
                                        continue;
                                    }

                                    // Decode the "n" text properties
                                    let n = match info.get_property("n") {
                                        Some(_n) => _n,
                                        None => continue,
                                    };

                                    // Parse the endpoint info
                                    let (dt, dn) = match parse_mdns_endpoint_info(n.val_str()) {
                                        Ok(r) => r,
                                        Err(_) => continue
                                    };

                                    let ip_port = format!("{ip}:{port}");
                                    let fullname = info.get_fullname().to_string();
                                    if TcpStream::connect(&ip_port).await.is_ok() {
                                        let ei = EndpointInfo {
                                            fullname: fullname.clone(),
                                            id: ip_port,
                                            name: Some(dn),
                                            ip: Some(ip.to_string()),
                                            port: Some(port.to_string()),
                                            rtype: Some(dt),
                                            present: Some(true),
                                            transport: Some(EndpointTransport::MdnsTcp),
                                            wifi_direct_peer_path: None,
                                            wifi_direct_peer_mac: None,
                                        };
                                        pending_removals.remove(&fullname);

                                        let changed = cache.get(&fullname) != Some(&ei);
                                        if changed {
                                            info!("ServiceResolved: Resolved a new service: {:?}", ei);
                                            cache.insert(fullname.clone(), ei.clone());
                                            let _ = self.sender.send(ei);
                                        } else {
                                            trace!("ServiceResolved: Ignoring unchanged service {}", fullname);
                                        }
                                    }
                                }
                                ServiceEvent::ServiceRemoved(_, fullname) => {
                                    trace!("ServiceRemoved: checking if should remove {}", fullname);
                                    if cache.contains_key(&fullname) {
                                        pending_removals.insert(fullname.clone(), Instant::now() + REMOVAL_GRACE_PERIOD);
                                        trace!(
                                            "ServiceRemoved: scheduling removal after grace period for {}",
                                            fullname
                                        );
                                    }
                                }
                                ServiceEvent::SearchStarted(_) | ServiceEvent::SearchStopped(_) => {}
                                _ => {}
                            }
                        },
                        Err(err) => error!("MDnsDiscovery: error: {}", err),
                    }
                }
            }
        }

        Ok(())
    }
}
